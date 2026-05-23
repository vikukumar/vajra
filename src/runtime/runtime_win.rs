#![no_std]
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use core::ffi::c_void;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
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
const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MB TLAB per thread

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
        let ptr = &G_THREAD_IDS[i] as *const u64 as *const AtomicU64;
        let actual_tid = (*ptr).load(Ordering::SeqCst);
        if actual_tid == tid {
            return i;
        }
    }
    
    // Register dynamically in empty slot
    for i in 0..MAX_THREADS {
        let ptr = &G_THREAD_IDS[i] as *const u64 as *mut AtomicU64;
        if (*ptr).compare_exchange(0, tid, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            G_STACK_BASES[i] = get_stack_base();
            let heap = VirtualAlloc(
                core::ptr::null_mut(),
                HEAP_SIZE,
                0x3000, // MEM_COMMIT | MEM_RESERVE
                0x04,   // PAGE_READWRITE
            );
            G_HEAP_STARTS[i] = heap as *mut u8;
            G_HEAP_LIMITS[i] = HEAP_SIZE;
            G_HEAP_BUMPS[i] = 0;
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
        buf[ptr] = b'0';
    } else {
        while abs_val > 0 {
            ptr -= 1;
            buf[ptr] = b'0' + (abs_val % 10) as u8;
            abs_val /= 10;
        }
    }

    if is_neg {
        ptr -= 1;
        buf[ptr] = b'-';
    }

    print_raw(buf.as_ptr().add(ptr), buf.len() - ptr);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_i64(val: i64) {
    print_i64_nn(val);
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
    G_THREAD_IDS[0] = tid;
    G_STACK_BASES[0] = get_stack_base();
    
    let heap = VirtualAlloc(
        core::ptr::null_mut(),
        HEAP_SIZE,
        0x3000, // MEM_COMMIT | MEM_RESERVE
        0x04,   // PAGE_READWRITE
    );
    G_HEAP_STARTS[0] = heap as *mut u8;
    G_HEAP_LIMITS[0] = HEAP_SIZE;
    G_HEAP_BUMPS[0] = 0;
}

#[no_mangle]
pub unsafe extern "C" fn vajra_alloc(size: usize) -> *mut u8 {
    let size = (size + 7) & !7;
    let total_size = size + 8;

    let idx = get_thread_idx();
    let bump = G_HEAP_BUMPS[idx];
    let limit = G_HEAP_LIMITS[idx];
    let start = G_HEAP_STARTS[idx];

    if start.is_null() {
        vajra_throw(b"Heap allocation failure\0".as_ptr());
    }

    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        G_HEAP_BUMPS[idx] = bump + total_size;
        return ptr.add(8);
    }

    gc_collect(idx);

    let bump = G_HEAP_BUMPS[idx];
    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        G_HEAP_BUMPS[idx] = bump + total_size;
        return ptr.add(8);
    }

    vajra_throw(b"Out of memory\0".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn vajra_free(_ptr: *mut u8) {
    // GC is automatic
}

unsafe fn mark_block(idx: usize, ptr: *mut u8) -> bool {
    let heap_start = G_HEAP_STARTS[idx];
    let heap_bump = G_HEAP_BUMPS[idx];

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
    let heap_start = G_HEAP_STARTS[idx];
    let heap_bump = G_HEAP_BUMPS[idx];

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
    let stack_base = G_STACK_BASES[idx] as *mut usize;

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

    G_HEAP_BUMPS[idx] = max_live_end;
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

    const UNINIT: core::mem::MaybeUninit<ThreadArg> = core::mem::MaybeUninit::uninit();
    let mut args = [UNINIT; 64];

    let mut _workers_spawned = 0;

    for t in 1..num_threads {
        let t_start = start + t as i64 * chunk_size;
        let t_end = if t as i64 == num_threads as i64 - 1 { end } else { t_start + chunk_size };
        if t_start >= end {
            join_counter.fetch_sub(1, Ordering::SeqCst);
            continue;
        }

        args[t as usize - 1].write(ThreadArg {
            start: t_start,
            end: t_end,
            context,
            loop_body,
            counter: &join_counter,
        });

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
            args[t as usize - 1].as_ptr() as *mut c_void,
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
