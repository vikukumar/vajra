#![no_std]
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use core::ffi::c_void;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        *s.add(i) = c as u8;
    }
    s
}

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        *dest.add(i) = *src.add(i);
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            return if a < b { -1 } else { 1 };
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

extern "system" {
    fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
    fn GetStdHandle(nStdHandle: i32) -> *mut c_void;
    fn WriteFile(
        hFile: *mut c_void,
        lpBuffer: *const u8,
        nNumberOfBytesToWrite: u32,
        lpNumberOfBytesWritten: *mut u32,
        lpOverlapped: *mut c_void,
    ) -> i32;
    fn ExitProcess(uExitCode: u32) -> !;
    fn VirtualAlloc(
        lpAddress: *mut c_void,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut c_void;
    #[allow(dead_code)]
    fn VirtualFree(
        lpAddress: *mut c_void,
        dwSize: usize,
        dwFreeType: u32,
    ) -> i32;
    fn CreateThread(
        lpThreadAttributes: *mut c_void,
        dwStackSize: usize,
        lpStartAddress: extern "system" fn(*mut c_void) -> u32,
        lpParameter: *mut c_void,
        dwCreationFlags: u32,
        lpThreadId: *mut u32,
    ) -> *mut c_void;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn GetSystemInfo(lpSystemInfo: *mut SYSTEM_INFO);
}

#[repr(C)]
struct SYSTEM_INFO {
    wProcessorArchitecture: u16,
    wReserved: u16,
    dwPageSize: u32,
    lpMinimumApplicationAddress: *mut c_void,
    lpMaximumApplicationAddress: *mut c_void,
    dwActiveProcessorMask: usize,
    dwNumberOfProcessors: u32,
    dwProcessorType: u32,
    dwAllocationGranularity: u32,
    wProcessorLevel: u16,
    wProcessorRevision: u16,
}

const MAX_THREADS: usize = 64;
const HEAP_SIZE: usize = 512 * 1024 * 1024; // 512 MB TLAB per thread

static mut G_STDOUT: *mut c_void = core::ptr::null_mut();
static mut G_THREAD_IDS: [u64; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_STARTS: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];
static mut G_HEAP_BUMPS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_LIMITS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_STACK_BASES: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];

// Removed G_THREAD_IDX_COUNTER to use lock-free slot scanning instead

#[repr(C)]
struct AllocHeader {
    size: u32,
    marked: u32,
}

unsafe fn get_thread_id() -> u64 {
    let thread_id: u64;
    core::arch::asm!(
        "mov {}, gs:[0x48]",
        out(reg) thread_id
    );
    thread_id
}

unsafe fn get_stack_base() -> *mut u8 {
    let stack_base: *mut u8;
    core::arch::asm!(
        "mov {}, gs:[0x08]",
        out(reg) stack_base
    );
    stack_base
}

unsafe fn get_thread_idx() -> usize {
    let tid = get_thread_id();
    for i in 0..MAX_THREADS {
        let ptr = G_THREAD_IDS.as_ptr().add(i) as *const AtomicU64;
        let actual_tid = (*ptr).load(Ordering::SeqCst);
        if actual_tid == tid {
            return i;
        }
    }
    
    // Register dynamically in empty slot
    for i in 0..MAX_THREADS {
        let ptr = G_THREAD_IDS.as_mut_ptr().add(i) as *mut AtomicU64;
        if (*ptr).compare_exchange(0, tid, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            *G_STACK_BASES.as_mut_ptr().add(i) = get_stack_base();
            let heap = VirtualAlloc(
                core::ptr::null_mut(),
                HEAP_SIZE,
                0x3000, // MEM_COMMIT | MEM_RESERVE
                0x04,   // PAGE_READWRITE
            );
            *G_HEAP_STARTS.as_mut_ptr().add(i) = heap as *mut u8;
            *G_HEAP_LIMITS.as_mut_ptr().add(i) = HEAP_SIZE;
            *G_HEAP_BUMPS.as_mut_ptr().add(i) = 0;
            return i;
        }
        if (*ptr).load(Ordering::SeqCst) == tid {
            return i;
        }
    }
    0
}

unsafe fn print_raw(buf: *const u8, len: usize) {
    let mut written: u32 = 0;
    if G_STDOUT.is_null() {
        G_STDOUT = GetStdHandle(-11);
    }
    WriteFile(G_STDOUT, buf, len as u32, &mut written, core::ptr::null_mut());
}

unsafe fn print_i64_nn(val: i64) {
    let mut buf = [0u8; 32];
    let mut ptr = buf.len();
    let is_neg = val < 0;
    let mut abs_val = if is_neg {
        if val == i64::MIN {
            print_raw(b"-9223372036854775808".as_ptr(), 20);
            return;
        }
        -val
    } else {
        val
    };

    if abs_val == 0 {
        ptr -= 1;
        *buf.as_mut_ptr().add(ptr) = b'0';
    } else {
        while abs_val > 0 {
            ptr -= 1;
            *buf.as_mut_ptr().add(ptr) = b'0' + (abs_val % 10) as u8;
            abs_val /= 10;
        }
    }

    if is_neg {
        ptr -= 1;
        *buf.as_mut_ptr().add(ptr) = b'-';
    }

    print_raw(buf.as_ptr().add(ptr), buf.len() - ptr);
}

#[inline]
fn is_tagged(val: u64) -> bool {
    (val & 1) == 1
}

#[inline]
fn untag(val: u64) -> i64 {
    (val as i64) >> 1
}

#[inline]
fn tag(val: i64) -> u64 {
    ((val << 1) as u64) | 1
}

#[repr(C)]
struct BigInt {
    sign: i64,
    len: i64,
    digits: *mut u64,
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_i64(val: i64) {
    let uval = val as u64;
    if is_tagged(uval) {
        print_i64_nn(untag(uval));
    } else {
        let b = uval as *const BigInt;
        if b.is_null() {
            print_raw(b"null".as_ptr(), 4);
        } else if (*b).len == 0 {
            print_raw(b"0".as_ptr(), 1);
        } else {
            if (*b).sign < 0 {
                print_raw(b"-".as_ptr(), 1);
            }
            let msd_idx = (*b).len as usize - 1;
            print_i64_nn(*(*b).digits.add(msd_idx) as i64);
            for i in (0..msd_idx).rev() {
                let digit = *(*b).digits.add(i);
                let mut buf = [b'0'; 9];
                let mut temp = digit;
                let mut ptr = 9;
                while temp > 0 && ptr > 0 {
                    ptr -= 1;
                    *buf.as_mut_ptr().add(ptr) = b'0' + (temp % 10) as u8;
                    temp /= 10;
                }
                print_raw(buf.as_ptr(), 9);
            }
        }
    }
    print_raw(b"\r\n".as_ptr(), 2);
}

fn f64_is_nan(v: f64) -> bool {
    let bits = v.to_bits();
    (bits & 0x7FF0000000000000) == 0x7FF0000000000000 && (bits & 0x000FFFFFFFFFFFFF) != 0
}

fn f64_is_infinite(v: f64) -> bool {
    let bits = v.to_bits();
    (bits & 0x7FF0000000000000) == 0x7FF0000000000000 && (bits & 0x000FFFFFFFFFFFFF) == 0
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_f64(val: f64) {
    let mut v = val;
    if f64_is_nan(v) {
        print_raw(b"NaN\r\n".as_ptr(), 5);
        return;
    }
    if f64_is_infinite(v) {
        if v < 0.0 {
            print_raw(b"-inf\r\n".as_ptr(), 6);
        } else {
            print_raw(b"inf\r\n".as_ptr(), 5);
        }
        return;
    }
    if v < 0.0 {
        print_raw(b"-".as_ptr(), 1);
        v = -v;
    }
    let ipart = v as i64;
    print_i64_nn(ipart);
    print_raw(b".".as_ptr(), 1);
    let mut frac = v - (ipart as f64);
    for _ in 0..6 {
        frac *= 10.0;
        let digit = frac as i64;
        print_i64_nn(digit % 10);
        frac -= digit as f64;
    }
    print_raw(b"\r\n".as_ptr(), 2);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_str(s: *const u8) {
    if s.is_null() {
        print_raw(b"null\r\n".as_ptr(), 6);
        return;
    }
    let len = vajra_strlen(s);
    print_raw(s, len);
    print_raw(b"\r\n".as_ptr(), 2);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_auto(s: *const u8) {
    vajra_print_str(s);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_exit(code: i64) -> ! {
    ExitProcess(code as u32);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_throw(msg: *const u8) -> ! {
    print_raw(b"Exception: ".as_ptr(), 11);
    vajra_print_str(msg);
    vajra_exit(1);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_spawn(
    _func: extern "C" fn(*mut c_void) -> u32,
    _arg: *mut c_void,
) -> *mut c_void {
    // Basic spawning stub, but we focus on vajra_parallel_for
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_spawn_val(
    _func: extern "C" fn(*mut c_void) -> u32,
    _arg: *mut c_void,
) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_readline() -> *mut u8 {
    // Readline placeholder: return null for now
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_runtime_init() {
    SetConsoleOutputCP(65001);
    G_STDOUT = GetStdHandle(-11);
    
    // Initialize main thread state
    let tid = get_thread_id();
    *G_THREAD_IDS.as_mut_ptr().add(0) = tid;
    *G_STACK_BASES.as_mut_ptr().add(0) = get_stack_base();
    
    let heap = VirtualAlloc(
        core::ptr::null_mut(),
        HEAP_SIZE,
        0x3000, // MEM_COMMIT | MEM_RESERVE
        0x04,   // PAGE_READWRITE
    );
    *G_HEAP_STARTS.as_mut_ptr().add(0) = heap as *mut u8;
    *G_HEAP_LIMITS.as_mut_ptr().add(0) = HEAP_SIZE;
    *G_HEAP_BUMPS.as_mut_ptr().add(0) = 0;
}

#[no_mangle]
pub unsafe extern "C" fn vajra_alloc(size: usize) -> *mut u8 {
    let size = (size + 7) & !7;
    let total_size = size + 8;

    let idx = get_thread_idx();
    let bump = *G_HEAP_BUMPS.as_ptr().add(idx);
    let limit = *G_HEAP_LIMITS.as_ptr().add(idx);
    let start = *G_HEAP_STARTS.as_ptr().add(idx);

    if start.is_null() {
        vajra_throw(b"Heap allocation failure\0".as_ptr());
    }

    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        *G_HEAP_BUMPS.as_mut_ptr().add(idx) = bump + total_size;
        return ptr.add(8);
    }

    gc_collect(idx);

    let bump = *G_HEAP_BUMPS.as_ptr().add(idx);
    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        *G_HEAP_BUMPS.as_mut_ptr().add(idx) = bump + total_size;
        return ptr.add(8);
    }

    vajra_throw(b"Out of memory\0".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn vajra_free(_ptr: *mut u8) {
    // GC is automatic
}

unsafe fn mark_block(idx: usize, ptr: *mut u8) -> bool {
    let heap_start = *G_HEAP_STARTS.as_ptr().add(idx);
    let heap_bump = *G_HEAP_BUMPS.as_ptr().add(idx);

    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        let block_size = (*header).size as usize;
        let block_start = heap_start.add(offset + 8);
        let block_end = block_start.add(block_size);

        if ptr >= block_start && ptr < block_end {
            if (*header).marked == 0 {
                (*header).marked = 1;
                return true;
            }
            return false;
        }
        offset += block_size + 8;
    }
    false
}

unsafe fn gc_collect(idx: usize) {
    let heap_start = *G_HEAP_STARTS.as_ptr().add(idx);
    let heap_bump = *G_HEAP_BUMPS.as_ptr().add(idx);

    // 1. Clear marks
    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        (*header).marked = 0;
        offset += (*header).size as usize + 8;
    }

    // Spill non-volatile registers to stack before scanning
    let mut regs = [0usize; 7];
    core::arch::asm!(
        "mov [{0}], rbx",
        "mov [{0} + 8], rsi",
        "mov [{0} + 16], rdi",
        "mov [{0} + 24], r12",
        "mov [{0} + 32], r13",
        "mov [{0} + 40], r14",
        "mov [{0} + 48], r15",
        in(reg) regs.as_mut_ptr(),
    );

    // 2. Scan stack
    let mut rsp: *mut usize;
    core::arch::asm!("mov {}, rsp", out(reg) rsp);
    let stack_base = *G_STACK_BASES.as_ptr().add(idx) as *mut usize;

    if !stack_base.is_null() && rsp < stack_base {
        let mut cur = rsp;
        while cur < stack_base {
            let val = *cur;
            let ptr_val = val as *mut u8;
            if ptr_val >= heap_start && ptr_val < heap_start.add(heap_bump) {
                mark_block(idx, ptr_val);
            }
            cur = cur.add(1);
        }
    }

    // 3. Transitive mark
    let mut changed = true;
    while changed {
        changed = false;
        let mut offset = 0;
        while offset < heap_bump {
            let header = heap_start.add(offset) as *mut AllocHeader;
            if (*header).marked == 1 {
                let size = (*header).size as usize;
                let payload = heap_start.add(offset + 8) as *mut usize;
                let num_words = size / 8;
                for i in 0..num_words {
                    let val = *payload.add(i);
                    let ptr_val = val as *mut u8;
                    if ptr_val >= heap_start && ptr_val < heap_start.add(heap_bump) {
                        if mark_block(idx, ptr_val) {
                            changed = true;
                        }
                    }
                }
            }
            offset += (*header).size as usize + 8;
        }
    }

    // 4. Sweep (high-watermark)
    let mut max_live_end = 0;
    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        let block_size = (*header).size as usize + 8;
        if (*header).marked == 1 {
            max_live_end = offset + block_size;
        }
        offset += block_size;
    }

    *G_HEAP_BUMPS.as_mut_ptr().add(idx) = max_live_end;
}

struct ThreadArg {
    start: i64,
    end: i64,
    context: *mut c_void,
    loop_body: extern "C" fn(i64, i64, *mut c_void),
    counter: *const AtomicI32,
}

#[no_mangle]
pub unsafe extern "C" fn vajra_parallel_for(
    start: i64,
    end: i64,
    context: *mut c_void,
    loop_body: extern "C" fn(i64, i64, *mut c_void),
) {
    let range = end - start;
    if range <= 0 {
        return;
    }

    const MIN_ITERATIONS_PER_THREAD: i64 = 100;
    
    let mut sys_info = core::mem::MaybeUninit::<SYSTEM_INFO>::uninit();
    GetSystemInfo(sys_info.as_mut_ptr());
    let sys_info = sys_info.assume_init();
    let num_cores = sys_info.dwNumberOfProcessors as usize;
    let num_cores = if num_cores == 0 { 4 } else { num_cores };
    let max_threads = if num_cores > MAX_THREADS { MAX_THREADS } else { num_cores };

    let num_threads = if range < MIN_ITERATIONS_PER_THREAD {
        1
    } else {
        let t = (range + MIN_ITERATIONS_PER_THREAD - 1) / MIN_ITERATIONS_PER_THREAD;
        if t as usize > max_threads {
            max_threads
        } else {
            t as usize
        }
    };

    if num_threads <= 1 {
        loop_body(start, end, context);
        return;
    }

    let chunk_size = (range + num_threads as i64 - 1) / num_threads as i64;
    let join_counter = AtomicI32::new((num_threads - 1) as i32);

    let args = vajra_alloc(64 * core::mem::size_of::<ThreadArg>()) as *mut ThreadArg;

    let mut _workers_spawned = 0;

    for t in 1..num_threads {
        let t_start = start + t as i64 * chunk_size;
        let t_end = if t as i64 == num_threads as i64 - 1 { end } else { t_start + chunk_size };
        if t_start >= end {
            join_counter.fetch_sub(1, Ordering::SeqCst);
            continue;
        }

        let arg_ptr = args.add(t as usize - 1);
        (*arg_ptr).start = t_start;
        (*arg_ptr).end = t_end;
        (*arg_ptr).context = context;
        (*arg_ptr).loop_body = loop_body;
        (*arg_ptr).counter = &join_counter;

        extern "system" fn worker_proc(param: *mut c_void) -> u32 {
            unsafe {
                let arg = &*(param as *const ThreadArg);
                
                // Auto register thread
                get_thread_idx();

                (arg.loop_body)(arg.start, arg.end, arg.context);

                (*arg.counter).fetch_sub(1, Ordering::SeqCst);
                0
            }
        }

        let handle = CreateThread(
            core::ptr::null_mut(),
            0,
            worker_proc,
            arg_ptr as *mut c_void,
            0,
            core::ptr::null_mut(),
        );

        if !handle.is_null() {
            CloseHandle(handle);
            _workers_spawned += 1;
        } else {
            (loop_body)(t_start, t_end, context);
            join_counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    // Main thread execution
    let main_end = if chunk_size < range { start + chunk_size } else { end };
    (loop_body)(start, main_end, context);

    // Spin wait
    while join_counter.load(Ordering::SeqCst) > 0 {
        core::hint::spin_loop();
    }
}

// --- BigInt Implementation ---

unsafe fn alloc_bigint(len: usize) -> *mut BigInt {
    let b = vajra_alloc(core::mem::size_of::<BigInt>()) as *mut BigInt;
    if len > 0 {
        let d = vajra_alloc(len * core::mem::size_of::<u64>()) as *mut u64;
        (*b).digits = d;
    } else {
        (*b).digits = core::ptr::null_mut();
    }
    (*b).len = len as i64;
    (*b).sign = 1;
    b
}

unsafe fn bigint_normalize(b: *mut BigInt) {
    let mut len = (*b).len as usize;
    while len > 0 && *(*b).digits.add(len - 1) == 0 {
        len -= 1;
    }
    (*b).len = len as i64;
    if len == 0 {
        (*b).sign = 1;
    }
}

unsafe fn bigint_from_i64(val: i64) -> *mut BigInt {
    if val == 0 {
        let b = alloc_bigint(0);
        (*b).sign = 1;
        return b;
    }
    let sign = if val < 0 { -1 } else { 1 };
    let abs_val = if val < 0 {
        if val == i64::MIN {
            9223372036854775808u64
        } else {
            (-val) as u64
        }
    } else {
        val as u64
    };
    
    let mut temp = abs_val;
    let mut len = 0;
    while temp > 0 {
        len += 1;
        temp /= 1_000_000_000;
    }
    
    let b = alloc_bigint(len);
    (*b).sign = sign;
    let mut temp = abs_val;
    for i in 0..len {
        *((*b).digits.add(i)) = temp % 1_000_000_000;
        temp /= 1_000_000_000;
    }
    b
}

unsafe fn bigint_to_i64(b: *const BigInt) -> Option<i64> {
    if (*b).len == 0 {
        return Some(0);
    }
    if (*b).len > 3 {
        return None;
    }
    let mut val = 0u64;
    let mut mul = 1u64;
    for i in 0..((*b).len as usize) {
        let digit = *(*b).digits.add(i);
        if i > 0 {
            mul = mul.checked_mul(1_000_000_000)?;
        }
        let term = digit.checked_mul(mul)?;
        val = val.checked_add(term)?;
    }
    if (*b).sign < 0 {
        if val > (i64::MIN as u64).wrapping_neg() {
            None
        } else {
            Some(-(val as i64))
        }
    } else {
        if val > i64::MAX as u64 {
            None
        } else {
            Some(val as i64)
        }
    }
}

unsafe fn bigint_cmp_abs(a: *const BigInt, b: *const BigInt) -> i32 {
    if (*a).len != (*b).len {
        return if (*a).len < (*b).len { -1 } else { 1 };
    }
    let len = (*a).len as usize;
    for i in (0..len).rev() {
        let da = *(*a).digits.add(i);
        let db = *(*b).digits.add(i);
        if da != db {
            return if da < db { -1 } else { 1 };
        }
    }
    0
}

unsafe fn bigint_add_abs(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    let max_len = if len_a > len_b { len_a } else { len_b };
    let res = alloc_bigint(max_len + 1);
    
    let mut carry = 0u64;
    for i in 0..max_len {
        let da = if i < len_a { *(*a).digits.add(i) } else { 0 };
        let db = if i < len_b { *(*b).digits.add(i) } else { 0 };
        let sum = da + db + carry;
        *(*res).digits.add(i) = sum % 1_000_000_000;
        carry = sum / 1_000_000_000;
    }
    if carry > 0 {
        *(*res).digits.add(max_len) = carry;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_sub_abs(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    let res = alloc_bigint(len_a);
    
    let mut borrow = 0i64;
    for i in 0..len_a {
        let da = *(*a).digits.add(i) as i64;
        let db = if i < len_b { *(*b).digits.add(i) as i64 } else { 0 };
        let mut diff = da - db - borrow;
        if diff < 0 {
            diff += 1_000_000_000;
            borrow = 1;
        } else {
            borrow = 0;
        }
        *(*res).digits.add(i) = diff as u64;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_add(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    if (*a).sign == (*b).sign {
        let res = bigint_add_abs(a, b);
        (*res).sign = (*a).sign;
        res
    } else {
        let cmp = bigint_cmp_abs(a, b);
        if cmp == 0 {
            alloc_bigint(0)
        } else if cmp > 0 {
            let res = bigint_sub_abs(a, b);
            (*res).sign = (*a).sign;
            res
        } else {
            let res = bigint_sub_abs(b, a);
            (*res).sign = (*b).sign;
            res
        }
    }
}

unsafe fn bigint_sub(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    if (*a).sign != (*b).sign {
        let res = bigint_add_abs(a, b);
        (*res).sign = (*a).sign;
        res
    } else {
        let cmp = bigint_cmp_abs(a, b);
        if cmp == 0 {
            alloc_bigint(0)
        } else if cmp > 0 {
            let res = bigint_sub_abs(a, b);
            (*res).sign = (*a).sign;
            res
        } else {
            let res = bigint_sub_abs(b, a);
            (*res).sign = -(*a).sign;
            res
        }
    }
}

unsafe fn bigint_mul(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    if len_a == 0 || len_b == 0 {
        return alloc_bigint(0);
    }
    
    let res = alloc_bigint(len_a + len_b);
    for i in 0..(len_a + len_b) {
        *(*res).digits.add(i) = 0;
    }
    
    for i in 0..len_a {
        let da = *(*a).digits.add(i);
        let mut carry = 0u64;
        for j in 0..len_b {
            let db = *(*b).digits.add(j);
            let current = *(*res).digits.add(i + j);
            let prod = da.wrapping_mul(db);
            let sum = prod.wrapping_add(carry).wrapping_add(current);
            *(*res).digits.add(i + j) = sum % 1_000_000_000;
            carry = sum / 1_000_000_000;
        }
        if carry > 0 {
            *(*res).digits.add(i + len_b) = carry;
        }
    }
    (*res).sign = (*a).sign * (*b).sign;
    bigint_normalize(res);
    res
}

unsafe fn bigint_shl_1(a: *const BigInt) -> *mut BigInt {
    let len = (*a).len as usize;
    let res = alloc_bigint(len + 1);
    let mut carry = 0u64;
    for i in 0..len {
        let val = (*(*a).digits.add(i) << 1) + carry;
        *(*res).digits.add(i) = val % 1_000_000_000;
        carry = val / 1_000_000_000;
    }
    if carry > 0 {
        *(*res).digits.add(len) = carry;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_shr_1(a: *const BigInt) -> *mut BigInt {
    let len = (*a).len as usize;
    if len == 0 {
        return alloc_bigint(0);
    }
    let res = alloc_bigint(len);
    let mut carry = 0u64;
    for i in (0..len).rev() {
        let val = *(*a).digits.add(i) + carry * 1_000_000_000;
        *(*res).digits.add(i) = val >> 1;
        carry = val & 1;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_div_rem_abs(a: *const BigInt, b: *const BigInt) -> (*mut BigInt, *mut BigInt) {
    if (*b).len == 0 {
        vajra_throw(b"Division by zero\0".as_ptr());
    }
    
    let cmp = bigint_cmp_abs(a, b);
    if cmp < 0 {
        let q = alloc_bigint(0);
        let r = alloc_bigint((*a).len as usize);
        (*r).sign = 1;
        for i in 0..((*a).len as usize) {
            *(*r).digits.add(i) = *(*a).digits.add(i);
        }
        return (q, r);
    }
    if cmp == 0 {
        let q = bigint_from_i64(1);
        let r = alloc_bigint(0);
        return (q, r);
    }
    
    let max_powers = 2048;
    let b_powers = vajra_alloc(max_powers * 8) as *mut *mut BigInt;
    for i in 0..max_powers {
        *b_powers.add(i) = core::ptr::null_mut();
    }
    
    *b_powers = alloc_bigint((*b).len as usize);
    for i in 0..((*b).len as usize) {
        *(*(*b_powers)).digits.add(i) = *(*b).digits.add(i);
    }
    
    let mut k = 0;
    let mut current_b = *b_powers;
    loop {
        let next_b = bigint_shl_1(current_b);
        if bigint_cmp_abs(next_b, a) > 0 {
            break;
        }
        k += 1;
        if k >= max_powers {
            break;
        }
        *b_powers.add(k) = next_b;
        current_b = next_b;
    }
    
    let mut rem = alloc_bigint((*a).len as usize);
    (*rem).sign = 1;
    for i in 0..((*a).len as usize) {
        *(*rem).digits.add(i) = *(*a).digits.add(i);
    }
    
    let mut quot = alloc_bigint(0);
    
    for i in (0..=k).rev() {
        let new_quot = bigint_shl_1(quot);
        quot = new_quot;
        
        let bp = *b_powers.add(i);
        if bigint_cmp_abs(rem, bp) >= 0 {
            let new_rem = bigint_sub_abs(rem, bp);
            let one = bigint_from_i64(1);
            let new_quot_plus = bigint_add_abs(quot, one);
            quot = new_quot_plus;
            rem = new_rem;
        }
    }
    
    (quot, rem)
}

unsafe fn bigint_div_rem(a: *const BigInt, b: *const BigInt) -> (*mut BigInt, *mut BigInt) {
    let (q, r) = bigint_div_rem_abs(a, b);
    (*q).sign = (*a).sign * (*b).sign;
    (*r).sign = (*a).sign;
    bigint_normalize(q);
    bigint_normalize(r);
    (q, r)
}

unsafe fn bigint_cmp(a: *const BigInt, b: *const BigInt) -> i64 {
    if (*a).sign != (*b).sign {
        return if (*a).sign < (*b).sign { -1 } else { 1 };
    }
    let cmp = bigint_cmp_abs(a, b);
    if (*a).sign < 0 {
        -cmp as i64
    } else {
        cmp as i64
    }
}

// --- Runtime Helper Functions ---

#[no_mangle]
pub unsafe extern "C" fn vajra_bigint_from_digits(
    digits_ptr: *const u64,
    len: i64,
    sign: i64,
) -> *mut BigInt {
    let b = alloc_bigint(len as usize);
    (*b).sign = sign;
    for i in 0..(len as usize) {
        *((*b).digits.add(i)) = *digits_ptr.add(i);
    }
    b
}

#[no_mangle]
pub unsafe extern "C" fn vajra_add(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(sum) = va.checked_add(vb) {
            if sum >= -4611686018427387904 && sum <= 4611686018427387903 {
                return tag(sum);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_add(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_sub(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(diff) = va.checked_sub(vb) {
            if diff >= -4611686018427387904 && diff <= 4611686018427387903 {
                return tag(diff);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_sub(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_mul(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(prod) = va.checked_mul(vb) {
            if prod >= -4611686018427387904 && prod <= 4611686018427387903 {
                return tag(prod);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_mul(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_div(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if vb == 0 {
            vajra_throw(b"Division by zero\0".as_ptr());
        }
        if let Some(quot) = va.checked_div(vb) {
            if quot >= -4611686018427387904 && quot <= 4611686018427387903 {
                return tag(quot);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let (q, _r) = bigint_div_rem(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(q) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    q as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_rem(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if vb == 0 {
            vajra_throw(b"Division by zero\0".as_ptr());
        }
        if let Some(rem) = va.checked_rem(vb) {
            if rem >= -4611686018427387904 && rem <= 4611686018427387903 {
                return tag(rem);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let (_q, r) = bigint_div_rem(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(r) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    r as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_cmp(a: u64, b: u64) -> i64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if va < vb { -1 } else if va > vb { 1 } else { 0 }
    } else {
        let bigint_a = if is_tagged(a) {
            bigint_from_i64(untag(a))
        } else {
            a as *mut BigInt
        };
        let bigint_b = if is_tagged(b) {
            bigint_from_i64(untag(b))
        } else {
            b as *mut BigInt
        };
        bigint_cmp(bigint_a, bigint_b)
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_not(a: u64) -> u64 {
    if is_tagged(a) {
        let val = untag(a);
        if val == 0 { 3 } else { 1 }
    } else {
        1
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_and(a: u64, b: u64) -> u64 {
    let val_a = if is_tagged(a) { untag(a) != 0 } else { true };
    let val_b = if is_tagged(b) { untag(b) != 0 } else { true };
    if val_a && val_b { 3 } else { 1 }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_or(a: u64, b: u64) -> u64 {
    let val_a = if is_tagged(a) { untag(a) != 0 } else { true };
    let val_b = if is_tagged(b) { untag(b) != 0 } else { true };
    if val_a || val_b { 3 } else { 1 }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_untag(a: u64) -> i64 {
    if is_tagged(a) {
        untag(a)
    } else {
        let b = a as *const BigInt;
        if b.is_null() || (*b).len == 0 {
            return 0;
        }
        let mut val = 0u64;
        let mut mul = 1u64;
        for i in 0..core::cmp::min((*b).len as usize, 3) {
            let digit = *(*b).digits.add(i);
            val = val.wrapping_add(digit.wrapping_mul(mul));
            mul = mul.wrapping_mul(1_000_000_000);
        }
        if (*b).sign < 0 {
            -(val as i64)
        } else {
            val as i64
        }
    }
}

