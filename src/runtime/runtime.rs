#![no_std]
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use core::ffi::c_void;

#[cfg(target_os = "windows")]
const EOL: &[u8] = b"\r\n";
#[cfg(not(target_os = "windows"))]
const EOL: &[u8] = b"\n";

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

#[cfg(target_os = "windows")]
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
    fn ReadFile(
        hFile: *mut c_void,
        lpBuffer: *mut u8,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead: *mut u32,
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
    fn GetCommandLineA() -> *const u8;
}

#[cfg(target_os = "windows")]
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

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_write(fd: i32, buf: *const u8, count: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 1, // SYS_write
        in("rdi") fd,
        in("rsi") buf,
        in("rdx") count,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_write(_fd: i32, _buf: *const u8, _count: usize) -> isize {
    0
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_mmap(
    addr: *mut c_void,
    len: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut c_void {
    let ret: *mut c_void;
    core::arch::asm!(
        "syscall",
        in("rax") 9, // SYS_mmap
        in("rdi") addr,
        in("rsi") len,
        in("rdx") prot,
        in("r10") flags,
        in("r8") fd,
        in("r9") offset,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_open(path: *const u8, flags: i32, mode: i32) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 2, // SYS_open
        in("rdi") path,
        in("rsi") flags,
        in("rdx") mode,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_read(fd: isize, buf: *mut u8, count: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 0, // SYS_read
        in("rdi") fd,
        in("rsi") buf,
        in("rdx") count,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_close(fd: isize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 3, // SYS_close
        in("rdi") fd,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_open(_path: *const u8, _flags: i32, _mode: i32) -> isize { 0 }
#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_read(_fd: isize, _buf: *mut u8, _count: usize) -> isize { 0 }
#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_close(_fd: isize) -> isize { 0 }

#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_mmap(
    _addr: *mut c_void,
    _len: usize,
    _prot: i32,
    _flags: i32,
    _fd: i32,
    _offset: i64,
) -> *mut c_void {
    core::ptr::null_mut()
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_exit(status: i32) -> ! {
    core::arch::asm!(
        "syscall",
        in("rax") 60, // SYS_exit
        in("rdi") status,
        options(noreturn),
    );
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "x86_64")))]
unsafe fn sys_exit(_status: i32) -> ! {
    loop {}
}

const MAX_THREADS: usize = 64;
const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MB TLAB per thread
const MAX_BIGINT_LIMBS: usize = 4096;
const MAX_BIGINT_MUL_WORK: usize = 1_000_000;

#[cfg(target_os = "windows")]
static mut G_STDOUT: *mut c_void = core::ptr::null_mut();
#[cfg(target_os = "windows")]
static mut G_STDIN: *mut c_void = core::ptr::null_mut();

static mut G_THREAD_IDS: [u64; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_STARTS: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];
static mut G_HEAP_BUMPS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_LIMITS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_STACK_BASES: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];

// Removed G_THREAD_IDX_COUNTER to use lock-free slot scanning instead

#[cfg(target_os = "windows")]
fn allocation_failed(ptr: *mut c_void) -> bool {
    ptr.is_null()
}

#[cfg(not(target_os = "windows"))]
fn allocation_failed(ptr: *mut c_void) -> bool {
    ptr.is_null() || ptr as isize == -1
}

#[repr(C)]
struct AllocHeader {
    size: u32,
    marked: u32,
}

#[cfg(target_os = "windows")]
unsafe fn get_thread_id() -> u64 {
    let thread_id: u64;
    core::arch::asm!(
        "mov {}, gs:[0x48]",
        out(reg) thread_id
    );
    thread_id
}

#[cfg(target_os = "windows")]
unsafe fn get_stack_base() -> *mut u8 {
    let stack_base: *mut u8;
    core::arch::asm!(
        "mov {}, gs:[0x08]",
        out(reg) stack_base
    );
    stack_base
}

#[cfg(target_os = "windows")]
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
            if allocation_failed(heap) {
                continue;
            }
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

#[cfg(target_os = "windows")]
unsafe fn print_raw(buf: *const u8, len: usize) {
    let mut written: u32 = 0;
    if G_STDOUT.is_null() {
        G_STDOUT = GetStdHandle(-11);
    }
    WriteFile(G_STDOUT, buf, len as u32, &mut written, core::ptr::null_mut());
}

#[cfg(target_os = "windows")]
unsafe fn read_raw(buf: *mut u8, len: usize) -> usize {
    let mut read: u32 = 0;
    if G_STDIN.is_null() {
        G_STDIN = GetStdHandle(-10);
    }
    if ReadFile(G_STDIN, buf, len as u32, &mut read, core::ptr::null_mut()) == 0 {
        0
    } else {
        read as usize
    }
}

#[cfg(not(target_os = "windows"))]
unsafe fn get_thread_id() -> u64 {
    1
}

#[cfg(not(target_os = "windows"))]
unsafe fn get_stack_base() -> *mut u8 {
    core::ptr::null_mut()
}

#[cfg(not(target_os = "windows"))]
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
            let heap = sys_mmap(
                core::ptr::null_mut(),
                HEAP_SIZE,
                0x03,  // PROT_READ | PROT_WRITE = 0x3
                0x22,  // MAP_PRIVATE | MAP_ANONYMOUS = 0x22
                -1,
                0,
            );
            if allocation_failed(heap) {
                continue;
            }
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

#[cfg(not(target_os = "windows"))]
unsafe fn print_raw(buf: *const u8, len: usize) {
    sys_write(1, buf, len);
}

#[cfg(not(target_os = "windows"))]
unsafe fn read_raw(buf: *mut u8, len: usize) -> usize {
    let read = sys_read(0, buf, len);
    if read < 0 { 0 } else { read as usize }
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

unsafe fn print_bigint_nn(b: *const BigInt) {
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

#[no_mangle]
pub unsafe extern "C" fn vajra_print_i64(val: i64) {
    let uval = val as u64;
    if is_tagged(uval) {
        print_i64_nn(untag(uval));
    } else {
        let b = uval as *const BigInt;
        print_bigint_nn(b);
    }
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
        print_raw(b"NaN".as_ptr(), 3);
        return;
    }
    if f64_is_infinite(v) {
        if v < 0.0 {
            print_raw(b"-inf".as_ptr(), 4);
        } else {
            print_raw(b"inf".as_ptr(), 3);
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
        print_raw(b"null".as_ptr(), 4);
        print_raw(EOL.as_ptr(), EOL.len());
        return;
    }
    let len = vajra_strlen(s);
    print_raw(s, len);
    print_raw(EOL.as_ptr(), EOL.len());
}

unsafe fn is_heap_ptr(val: u64) -> bool {
    let ptr = val as *mut u8;
    for i in 0..MAX_THREADS {
        let start = G_HEAP_STARTS[i];
        if !start.is_null() {
            let bump = G_HEAP_BUMPS[i];
            if ptr >= start && ptr < start.add(bump) {
                return true;
            }
        }
    }
    false
}

unsafe fn is_valid_class_name_ptr(ptr: *const u8) -> bool {
    if ptr.is_null() { return false; }
    
    // Check if ptr is within the executable image (near vajra_runtime_init)
    let base = vajra_runtime_init as usize;
    let target = ptr as usize;
    let diff = if target > base { target - base } else { base - target };
    if diff > 10 * 1024 * 1024 { // 10 MB limit
        return false;
    }
    
    let mut len = 0;
    loop {
        let c = *ptr.add(len);
        if c == 0 { break; }
        if len == 0 {
            if !c.is_ascii_alphabetic() { return false; }
        } else {
            if !c.is_ascii_alphanumeric() && c != b'_' { return false; }
        }
        len += 1;
        if len > 50 { return false; }
    }
    len > 0
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_auto(val: u64) {
    if G_VERBOSE {
        print_raw(b"DBG print_auto: val=".as_ptr(), 20);
        print_i64_nn(val as i64);
        print_raw(EOL.as_ptr(), EOL.len());
    }

    if val == 0 {
        print_raw(b"null".as_ptr(), 4);
        return;
    }
    if is_tagged(val) {
        print_i64_nn(untag(val));
        return;
    }
    
    // If it's not on the heap, it's a string literal or global variable
    if !is_heap_ptr(val) {
        let len = vajra_strlen(val as *const u8);
        print_raw(val as *const u8, len);
        return;
    }
    
    let p = val as *const u64;
    let first = *p;
    if first == 1 || first == 1_i64 as u64 || first == -1_i64 as u64 {
        let b = val as *const BigInt;
        print_bigint_nn(b);
        return;
    }
    
    if is_valid_class_name_ptr(first as *const u8) {
        print_raw(b"[Object ".as_ptr(), 8);
        let len = vajra_strlen(first as *const u8);
        print_raw(first as *const u8, len);
        print_raw(b"]".as_ptr(), 1);
        return;
    }
    
    let len = vajra_strlen(val as *const u8);
    print_raw(val as *const u8, len);
}


#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn vajra_exit(code: i64) -> ! {
    ExitProcess(code as u32);
}

#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "C" fn vajra_exit(code: i64) -> ! {
    sys_exit(code as i32);
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
    const MAX_LINE: usize = 4096;
    let buf = vajra_alloc(MAX_LINE + 1);
    let mut n = read_raw(buf, MAX_LINE);
    while n > 0 {
        let c = *buf.add(n - 1);
        if c == b'\n' || c == b'\r' {
            n -= 1;
        } else {
            break;
        }
    }
    *buf.add(n) = 0;
    buf
}

static mut G_VERBOSE: bool = false;

#[cfg(target_os = "windows")]
unsafe fn init_verbosity() {
    let cmd = GetCommandLineA();
    if !cmd.is_null() {
        let mut len = 0;
        while *cmd.add(len) != 0 {
            len += 1;
        }
        let mut i = 0;
        while i < len {
            if i + 3 <= len {
                let c0 = *cmd.add(i);
                let c1 = *cmd.add(i + 1);
                let c2 = *cmd.add(i + 2);
                if c0 == b'-' && c1 == b'v' && c2 == b'v' {
                    let before_ok = i == 0 || *cmd.add(i - 1) == b' ' || *cmd.add(i - 1) == b'"';
                    let after_ok = i + 3 == len || *cmd.add(i + 3) == b' ' || *cmd.add(i + 3) == b'"';
                    if before_ok && after_ok {
                        G_VERBOSE = true;
                        break;
                    }
                }
            }
            i += 1;
        }
    }
}

#[cfg(not(target_os = "windows"))]
unsafe fn init_verbosity() {
    let mut buf = [0u8; 1024];
    let buf_ptr = buf.as_mut_ptr();
    let path = b"/proc/self/cmdline\0";
    let fd = sys_open(path.as_ptr(), 0, 0); // O_RDONLY = 0
    if fd >= 0 {
        let n = sys_read(fd, buf_ptr, 1024);
        sys_close(fd);
        if n > 0 {
            let mut i = 0;
            while i < n as usize {
                let mut arg_len = 0;
                while i + arg_len < n as usize && *buf_ptr.add(i + arg_len) != 0 {
                    arg_len += 1;
                }
                if arg_len == 3 {
                    let c0 = *buf_ptr.add(i);
                    let c1 = *buf_ptr.add(i + 1);
                    let c2 = *buf_ptr.add(i + 2);
                    if c0 == b'-' && c1 == b'v' && c2 == b'v' {
                        G_VERBOSE = true;
                        break;
                    }
                }
                i += arg_len + 1;
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn vajra_runtime_init() {
    init_verbosity();
    if G_VERBOSE {
        print_raw(b"DBG: vajra_runtime_init started\n\0".as_ptr(), 33);
    }
    SetConsoleOutputCP(65001);

    G_STDOUT = GetStdHandle(-11);
    G_STDIN = GetStdHandle(-10);
    
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
    if allocation_failed(heap) {
        vajra_throw(b"Heap allocation failure\0".as_ptr());
    }
    *G_HEAP_STARTS.as_mut_ptr().add(0) = heap as *mut u8;
    *G_HEAP_LIMITS.as_mut_ptr().add(0) = HEAP_SIZE;
    *G_HEAP_BUMPS.as_mut_ptr().add(0) = 0;
    if G_VERBOSE {
        print_raw(b"DBG: vajra_runtime_init finished\n\0".as_ptr(), 34);
    }
}

#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "C" fn vajra_runtime_init() {
    init_verbosity();
    if G_VERBOSE {
        print_raw(b"DBG: vajra_runtime_init started\n\0".as_ptr(), 33);
    }
    // Initialize main thread state
    let tid = get_thread_id();
    *G_THREAD_IDS.as_mut_ptr().add(0) = tid;
    *G_STACK_BASES.as_mut_ptr().add(0) = get_stack_base();
    
    let heap = sys_mmap(
        core::ptr::null_mut(),
        HEAP_SIZE,
        0x03,  // PROT_READ | PROT_WRITE
        0x22,  // MAP_PRIVATE | MAP_ANONYMOUS
        -1,
        0,
    );
    if allocation_failed(heap) {
        vajra_throw(b"Heap allocation failure\0".as_ptr());
    }
    *G_HEAP_STARTS.as_mut_ptr().add(0) = heap as *mut u8;
    *G_HEAP_LIMITS.as_mut_ptr().add(0) = HEAP_SIZE;
    *G_HEAP_BUMPS.as_mut_ptr().add(0) = 0;
    if G_VERBOSE {
        print_raw(b"DBG: vajra_runtime_init finished\n\0".as_ptr(), 34);
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_alloc(size: usize) -> *mut u8 {
    let Some(size) = size.checked_add(7) else {
        vajra_throw(b"Allocation size overflow\0".as_ptr());
    };
    let size = size & !7;
    let Some(total_size) = size.checked_add(8) else {
        vajra_throw(b"Allocation size overflow\0".as_ptr());
    };
    if total_size > HEAP_SIZE {
        vajra_throw(b"Allocation exceeds heap limit\0".as_ptr());
    }

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
    #[cfg(target_arch = "x86_64")]
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
    let mut rsp: *mut usize = core::ptr::null_mut();
    #[cfg(target_arch = "x86_64")]
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

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn vajra_parallel_for(
    start: i64,
    end: i64,
    context: *mut c_void,
    loop_body: extern "C" fn(i64, i64, *mut c_void),
) {
    let start_val = if is_tagged(start as u64) {
        untag(start as u64)
    } else if start == 0 {
        0
    } else {
        bigint_to_i64(start as *const BigInt).unwrap_or(0)
    };
    let end_val = if is_tagged(end as u64) {
        untag(end as u64)
    } else if end == 0 {
        0
    } else {
        bigint_to_i64(end as *const BigInt).unwrap_or(i64::MAX)
    };
    let range = end_val - start_val;
    if range <= 0 {
        return;
    }

    const MIN_ITERATIONS_PER_THREAD: i64 = 100_000;
    
    let mut sys_info = core::mem::MaybeUninit::<SYSTEM_INFO>::uninit();
    GetSystemInfo(sys_info.as_mut_ptr());
    let sys_info = sys_info.assume_init();
    let num_cores = sys_info.dwNumberOfProcessors as usize;
    let num_cores = if num_cores == 0 { 4 } else { num_cores };
    let max_threads = if num_cores > MAX_THREADS { MAX_THREADS } else { num_cores };

    let is_worker = get_thread_idx() > 0;
    let num_threads = if is_worker || range < MIN_ITERATIONS_PER_THREAD {
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
        loop_body(tag(start_val) as i64, tag(end_val) as i64, context);
        return;
    }

    let chunk_size = (range + num_threads as i64 - 1) / num_threads as i64;
    let join_counter = AtomicI32::new((num_threads - 1) as i32);

    let args = vajra_alloc(64 * core::mem::size_of::<ThreadArg>()) as *mut ThreadArg;

    let mut _workers_spawned = 0;

    for t in 1..num_threads {
        let t_start = start_val + t as i64 * chunk_size;
        let t_end = if t as i64 == num_threads as i64 - 1 { end_val } else { t_start + chunk_size };
        if t_start >= end_val {
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

                (arg.loop_body)(tag(arg.start) as i64, tag(arg.end) as i64, arg.context);

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
            (loop_body)(tag(t_start) as i64, tag(t_end) as i64, context);
            join_counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    // Main thread execution
    let main_end = if chunk_size < range { start_val + chunk_size } else { end_val };
    (loop_body)(tag(start_val) as i64, tag(main_end) as i64, context);

    // Spin wait
    while join_counter.load(Ordering::SeqCst) > 0 {
        core::hint::spin_loop();
    }
}

#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "C" fn vajra_parallel_for(
    start: i64,
    end: i64,
    context: *mut c_void,
    loop_body: extern "C" fn(i64, i64, *mut c_void),
) {
    let start_val = if is_tagged(start as u64) {
        untag(start as u64)
    } else if start == 0 {
        0
    } else {
        bigint_to_i64(start as *const BigInt).unwrap_or(0)
    };
    let end_val = if is_tagged(end as u64) {
        untag(end as u64)
    } else if end == 0 {
        0
    } else {
        bigint_to_i64(end as *const BigInt).unwrap_or(i64::MAX)
    };
    if end_val > start_val {
        loop_body(tag(start_val) as i64, tag(end_val) as i64, context);
    }
}

// --- BigInt Implementation ---

unsafe fn alloc_bigint(len: usize) -> *mut BigInt {
    if len > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt too large\0".as_ptr());
    }
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
    if max_len >= MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt addition exceeds limit\0".as_ptr());
    }
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
    let Some(result_len) = len_a.checked_add(len_b) else {
        vajra_throw(b"BigInt multiplication size overflow\0".as_ptr());
    };
    if result_len > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt multiplication exceeds limit\0".as_ptr());
    }
    let Some(work) = len_a.checked_mul(len_b) else {
        vajra_throw(b"BigInt multiplication work overflow\0".as_ptr());
    };
    if work > MAX_BIGINT_MUL_WORK {
        vajra_throw(b"BigInt multiplication work limit exceeded\0".as_ptr());
    }
    
    let res = alloc_bigint(result_len);
    for i in 0..result_len {
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
    if len >= MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt shift exceeds limit\0".as_ptr());
    }
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

unsafe fn cmp_i64_bigint(a: i64, b: *const BigInt) -> i64 {
    if (*b).len == 0 {
        return if a < 0 { -1 } else if a > 0 { 1 } else { 0 };
    }
    if a < 0 && (*b).sign > 0 {
        return -1;
    }
    if a >= 0 && (*b).sign < 0 {
        return 1;
    }
    if (*b).len > 3 {
        return if (*b).sign > 0 { -1 } else { 1 };
    }
    if let Some(b_val) = bigint_to_i64(b) {
        return if a < b_val { -1 } else if a > b_val { 1 } else { 0 };
    }
    if (*b).sign > 0 { -1 } else { 1 }
}

// --- Runtime Helper Functions ---

#[no_mangle]
pub unsafe extern "C" fn vajra_bigint_from_digits(
    digits_ptr: *const u64,
    len: i64,
    sign: i64,
) -> *mut BigInt {
    if len < 0 || len as usize > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt literal exceeds limit\0".as_ptr());
    }
    let b = alloc_bigint(len as usize);
    (*b).sign = sign;
    for i in 0..(len as usize) {
        *((*b).digits.add(i)) = *digits_ptr.add(i);
    }
    b
}

unsafe fn is_string_val(val: u64) -> bool {
    if val == 0 || is_tagged(val) {
        return false;
    }
    if is_heap_ptr(val) {
        let p = val as *const u64;
        let first = *p;
        if first == 1 || first == 1_i64 as u64 || first == -1_i64 as u64 {
            return false;
        }
        if is_valid_class_name_ptr(first as *const u8) {
            return false;
        }
    }
    true
}

unsafe fn int_to_str(val: i64) -> *mut u8 {
    let mut buf = [0u8; 32];
    let buf_ptr = buf.as_mut_ptr();
    let mut ptr = 32;
    let is_neg = val < 0;
    let mut abs_val = if is_neg {
        if val == i64::MIN {
            let res = vajra_alloc(21);
            memcpy(res, b"-9223372036854775808\0".as_ptr(), 21);
            return res;
        }
        -val
    } else {
        val
    };

    if abs_val == 0 {
        ptr -= 1;
        *buf_ptr.add(ptr) = b'0';
    } else {
        while abs_val > 0 {
            ptr -= 1;
            *buf_ptr.add(ptr) = b'0' + (abs_val % 10) as u8;
            abs_val /= 10;
        }
    }

    if is_neg {
        ptr -= 1;
        *buf_ptr.add(ptr) = b'-';
    }

    let len = 32 - ptr;
    let res = vajra_alloc(len + 1);
    memcpy(res, buf_ptr.add(ptr), len);
    *res.add(len) = 0;
    res
}

unsafe fn bigint_to_str(b: *const BigInt) -> *mut u8 {
    if b.is_null() {
        let res = vajra_alloc(5);
        memcpy(res, b"null\0".as_ptr(), 5);
        return res;
    }
    if (*b).len == 0 {
        let res = vajra_alloc(2);
        memcpy(res, b"0\0".as_ptr(), 2);
        return res;
    }

    let mut temp_buf = [0u8; 1024];
    let temp_ptr = temp_buf.as_mut_ptr();
    let mut temp_len = 0;

    let mut write_char = |c: u8| {
        if temp_len < 1023 {
            *temp_ptr.add(temp_len) = c;
            temp_len += 1;
        }
    };

    if (*b).sign < 0 {
        write_char(b'-');
    }

    let msd_idx = (*b).len as usize - 1;
    let mut msd_val = *(*b).digits.add(msd_idx) as i64;
    let mut digits = [0u8; 20];
    let digits_ptr = digits.as_mut_ptr();
    let mut d_ptr = 20;
    if msd_val == 0 {
        d_ptr -= 1;
        *digits_ptr.add(d_ptr) = b'0';
    } else {
        while msd_val > 0 {
            d_ptr -= 1;
            *digits_ptr.add(d_ptr) = b'0' + (msd_val % 10) as u8;
            msd_val /= 10;
        }
    }
    for k in d_ptr..20 {
        write_char(*digits_ptr.add(k));
    }

    for i in (0..msd_idx).rev() {
        let digit = *(*b).digits.add(i);
        let mut d_val = digit;
        let mut sub_digits = [b'0'; 9];
        let sub_ptr = sub_digits.as_mut_ptr();
        let mut sd_ptr = 9;
        while d_val > 0 && sd_ptr > 0 {
            sd_ptr -= 1;
            *sub_ptr.add(sd_ptr) = b'0' + (d_val % 10) as u8;
            d_val /= 10;
        }
        for k in 0..9 {
            write_char(*sub_ptr.add(k));
        }
    }

    let res = vajra_alloc(temp_len + 1);
    memcpy(res, temp_ptr, temp_len);
    *res.add(temp_len) = 0;
    res
}

unsafe fn val_to_str(val: u64) -> *mut u8 {
    if val == 0 {
        let res = vajra_alloc(5);
        memcpy(res, b"null\0".as_ptr(), 5);
        return res;
    }
    if is_tagged(val) {
        return int_to_str(untag(val));
    }
    if is_heap_ptr(val) {
        let p = val as *const u64;
        let first = *p;
        if first == 1 || first == 1_i64 as u64 || first == -1_i64 as u64 {
            return bigint_to_str(val as *const BigInt);
        }
        if is_valid_class_name_ptr(first as *const u8) {
            let name_len = vajra_strlen(first as *const u8);
            let total_len = 8 + name_len + 1;
            let res = vajra_alloc(total_len + 1);
            memcpy(res, b"[Object ".as_ptr(), 8);
            memcpy(res.add(8), first as *const u8, name_len);
            *res.add(8 + name_len) = b']';
            *res.add(8 + name_len + 1) = 0;
            return res;
        }
    }
    val as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn vajra_pow(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let base = untag(a);
        let exp = untag(b);
        if exp >= 0 {
            let mut res = 1i64;
            let mut mul = base;
            let mut temp_exp = exp;
            let mut overflow = false;
            while temp_exp > 0 {
                if temp_exp & 1 == 1 {
                    if let Some(r) = res.checked_mul(mul) {
                        res = r;
                    } else {
                        overflow = true;
                        break;
                    }
                }
                if temp_exp > 1 {
                    if let Some(m) = mul.checked_mul(mul) {
                        mul = m;
                    } else {
                        overflow = true;
                        break;
                    }
                }
                temp_exp >>= 1;
            }
            if !overflow {
                if res >= -4611686018427387904 && res <= 4611686018427387903 {
                    return tag(res);
                }
            }
        }
    }

    // Float pow fallback
    let base_f = if is_tagged(a) {
        untag(a) as f64
    } else {
        f64::from_bits(a)
    };
    let exp_f = if is_tagged(b) {
        untag(b) as f64
    } else {
        f64::from_bits(b)
    };
    let res_f = vajra_math_pow(base_f, exp_f);
    res_f.to_bits()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_add(a: u64, b: u64) -> u64 {
    if is_string_val(a) || is_string_val(b) {
        let str_a = val_to_str(a);
        let str_b = val_to_str(b);
        let len_a = vajra_strlen(str_a);
        let len_b = vajra_strlen(str_b);
        let res = vajra_alloc(len_a + len_b + 1);
        memcpy(res, str_a, len_a);
        memcpy(res.add(len_a), str_b, len_b);
        *res.add(len_a + len_b) = 0;
        return res as u64;
    }

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

unsafe fn is_valid_bigint(val: u64) -> bool {
    if !is_heap_ptr(val) {
        return false;
    }
    if (val & 7) != 0 {
        return false;
    }
    let header = (val - 8) as *const AllocHeader;
    if (*header).size < core::mem::size_of::<BigInt>() as u32 {
        return false;
    }
    let first = *(val as *const u64) as i64;
    first == 1 || first == -1
}

unsafe fn get_string_ptr(val: u64) -> *const u8 {
    if val == 0 {
        return core::ptr::null();
    }
    if is_tagged(val) {
        return core::ptr::null();
    }
    if is_heap_ptr(val) {
        if is_valid_bigint(val) {
            return core::ptr::null();
        }
        let header = (val - 8) as *const AllocHeader;
        if (*header).size >= 8 {
            let first = *(val as *const u64);
            if is_valid_class_name_ptr(first as *const u8) {
                return core::ptr::null();
            }
        }
        return val as *const u8;
    }
    val as *const u8
}

#[no_mangle]
pub unsafe extern "C" fn vajra_cmp(a: u64, b: u64) -> i64 {
    if a == b {
        return 0;
    }
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if va < vb { -1 } else if va > vb { 1 } else { 0 }
    } else {
        let is_a_num = is_tagged(a) || is_valid_bigint(a);
        let is_b_num = is_tagged(b) || is_valid_bigint(b);
        
        if is_a_num && is_b_num {
            if is_tagged(a) && is_tagged(b) {
                let va = untag(a);
                let vb = untag(b);
                if va < vb { -1 } else if va > vb { 1 } else { 0 }
            } else if is_tagged(a) {
                cmp_i64_bigint(untag(a), b as *const BigInt)
            } else if is_tagged(b) {
                -cmp_i64_bigint(untag(b), a as *const BigInt)
            } else {
                bigint_cmp(a as *const BigInt, b as *const BigInt)
            }
        } else {
            if a == 0 || b == 0 {
                return if a == 0 { -1 } else { 1 };
            }
            let str_a = get_string_ptr(a);
            let str_b = get_string_ptr(b);
            if str_a.is_null() || str_b.is_null() {
                if a < b { -1 } else { 1 }
            } else {
                let mut i = 0;
                loop {
                    let ca = *str_a.add(i);
                    let cb = *str_b.add(i);
                    if ca != cb {
                        return if ca < cb { -1 } else { 1 };
                    }
                    if ca == 0 {
                        break;
                    }
                    i += 1;
                }
                0
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn abs(n: i64) -> i64 {
    if is_tagged(n as u64) {
        let val = untag(n as u64);
        let abs_val = if val < 0 {
            if val == i64::MIN {
                i64::MAX
            } else {
                -val
            }
        } else {
            val
        };
        tag(abs_val) as i64
    } else {
        if n < 0 {
            if n == i64::MIN {
                i64::MAX
            } else {
                -n
            }
        } else {
            n
        }
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

// === Math, Random, DateTime, and Socket Runtime Features ===

#[repr(C)]
struct timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[cfg(target_os = "windows")]
extern "system" {
    fn LoadLibraryA(lpLibFileName: *const u8) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

#[cfg(target_os = "windows")]
static mut G_WINSOCK_INITIALIZED: bool = false;
#[cfg(target_os = "windows")]
static mut G_WSA_STARTUP: Option<unsafe extern "system" fn(u16, *mut u8) -> i32> = None;
#[cfg(target_os = "windows")]
static mut G_SOCKET: Option<unsafe extern "system" fn(i32, i32, i32) -> usize> = None;
#[cfg(target_os = "windows")]
static mut G_CONNECT: Option<unsafe extern "system" fn(usize, *const u8, i32) -> i32> = None;
#[cfg(target_os = "windows")]
static mut G_SEND: Option<unsafe extern "system" fn(usize, *const u8, i32, i32) -> i32> = None;
#[cfg(target_os = "windows")]
static mut G_RECV: Option<unsafe extern "system" fn(usize, *mut u8, i32, i32) -> i32> = None;
#[cfg(target_os = "windows")]
static mut G_CLOSESOCKET: Option<unsafe extern "system" fn(usize) -> i32> = None;

#[cfg(target_os = "windows")]
unsafe fn ensure_winsock() {
    if G_WINSOCK_INITIALIZED { return; }
    let ws2 = LoadLibraryA(b"ws2_32.dll\0".as_ptr());
    if ws2.is_null() {
        vajra_throw(b"Failed to load ws2_32.dll\0".as_ptr());
    }
    G_WSA_STARTUP = core::mem::transmute(GetProcAddress(ws2, b"WSAStartup\0".as_ptr()));
    G_SOCKET = core::mem::transmute(GetProcAddress(ws2, b"socket\0".as_ptr()));
    G_CONNECT = core::mem::transmute(GetProcAddress(ws2, b"connect\0".as_ptr()));
    G_SEND = core::mem::transmute(GetProcAddress(ws2, b"send\0".as_ptr()));
    G_RECV = core::mem::transmute(GetProcAddress(ws2, b"recv\0".as_ptr()));
    G_CLOSESOCKET = core::mem::transmute(GetProcAddress(ws2, b"closesocket\0".as_ptr()));

    let mut wsa_data = [0u8; 512];
    if let Some(wsa_startup) = G_WSA_STARTUP {
        wsa_startup(0x0202, wsa_data.as_mut_ptr());
    }
    G_WINSOCK_INITIALIZED = true;
}

#[cfg(target_os = "windows")]
static mut G_GET_SYSTEM_TIME_AS_FILE_TIME: Option<unsafe extern "system" fn(*mut u64)> = None;

#[cfg(target_os = "windows")]
unsafe fn ensure_datetime() {
    if G_GET_SYSTEM_TIME_AS_FILE_TIME.is_some() { return; }
    let k32 = LoadLibraryA(b"kernel32.dll\0".as_ptr());
    G_GET_SYSTEM_TIME_AS_FILE_TIME = core::mem::transmute(GetProcAddress(k32, b"GetSystemTimeAsFileTime\0".as_ptr()));
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_socket(domain: i32, ty: i32, protocol: i32) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 41, // SYS_socket
        in("rdi") domain,
        in("rsi") ty,
        in("rdx") protocol,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_connect(fd: isize, addr: *const u8, addrlen: u32) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 42, // SYS_connect
        in("rdi") fd,
        in("rsi") addr,
        in("rdx") addrlen,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_send(fd: isize, buf: *const u8, len: usize, flags: i32) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 44, // SYS_sendto
        in("rdi") fd,
        in("rsi") buf,
        in("rdx") len,
        in("r10") flags,
        in("r8") 0,
        in("r9") 0,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_recv(fd: isize, buf: *mut u8, len: usize, flags: i32) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 45, // SYS_recvfrom
        in("rdi") fd,
        in("rsi") buf,
        in("rdx") len,
        in("r10") flags,
        in("r8") 0,
        in("r9") 0,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_close(fd: isize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 3, // SYS_close
        in("rdi") fd,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
unsafe fn sys_clock_gettime(clk_id: i32, tp: *mut timespec) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        in("rax") 228, // SYS_clock_gettime
        in("rdi") clk_id,
        in("rsi") tp,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

unsafe fn parse_ipv4_to_bytes(ip_str: *const u8) -> [u8; 4] {
    let mut bytes = [0u8; 4];
    let mut current_val = 0u8;
    let mut byte_idx = 0;
    let mut i = 0;
    loop {
        let c = *ip_str.add(i);
        if c == 0 {
            if byte_idx < 4 {
                bytes[byte_idx] = current_val;
            }
            break;
        } else if c == b'.' {
            if byte_idx < 4 {
                bytes[byte_idx] = current_val;
                byte_idx += 1;
                current_val = 0;
            }
        } else if c >= b'0' && c <= b'9' {
            current_val = current_val.wrapping_mul(10).wrapping_add(c - b'0');
        }
        i += 1;
    }
    bytes
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_create() -> u64 {
    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(socket_fn) = G_SOCKET {
            let sock = socket_fn(2, 1, 6);
            if sock == usize::MAX {
                return tag(-1);
            }
            return tag(sock as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let sock = sys_socket(2, 1, 6);
            tag(sock as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_connect(sock_val: u64, ip_val: u64, port_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let ip_ptr = ip_val as *const u8;
    let port = untag(port_val) as u32;

    let bytes = parse_ipv4_to_bytes(ip_ptr);

    let mut addr = [0u8; 16];
    let family_val = 2u16;
    core::ptr::copy_nonoverlapping(&family_val as *const u16 as *const u8, addr.as_mut_ptr(), 2);
    let port_bytes = [(port >> 8) as u8, (port & 0xFF) as u8];
    core::ptr::copy_nonoverlapping(port_bytes.as_ptr(), addr.as_mut_ptr().add(2), 2);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr.as_mut_ptr().add(4), 4);

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(connect_fn) = G_CONNECT {
            let res = connect_fn(sock, addr.as_ptr(), 16);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_connect(sock as isize, addr.as_ptr(), 16);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_send(sock_val: u64, data_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let data_ptr = data_val as *const u8;
    let len = vajra_strlen(data_ptr);

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(send_fn) = G_SEND {
            let res = send_fn(sock, data_ptr, len as i32, 0);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_send(sock as isize, data_ptr, len, 0);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_recv(sock_val: u64, len_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let len = untag(len_val) as usize;

    let buf = vajra_alloc(len + 1);
    let mut bytes_received: isize = -1;

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(recv_fn) = G_RECV {
            let res = recv_fn(sock, buf, len as i32, 0);
            bytes_received = res as isize;
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            bytes_received = sys_recv(sock as isize, buf, len, 0);
        }
    }

    if bytes_received <= 0 {
        *buf = 0;
    } else {
        *buf.add(bytes_received as usize) = 0;
    }
    buf as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_close(sock_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(close_fn) = G_CLOSESOCKET {
            let res = close_fn(sock);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_close(sock as isize);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_datetime_now() -> u64 {
    #[cfg(target_os = "windows")]
    {
        ensure_datetime();
        if let Some(sys_time_fn) = G_GET_SYSTEM_TIME_AS_FILE_TIME {
            let mut file_time = 0u64;
            sys_time_fn(&mut file_time);
            let unix_seconds = (file_time - 116444736000000000) / 10000000;
            return tag(unix_seconds as i64);
        }
        tag(0)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let mut tp = timespec { tv_sec: 0, tv_nsec: 0 };
            sys_clock_gettime(0, &mut tp);
            tag(tp.tv_sec)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(0)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_datetime_epoch() -> u64 {
    vajra_datetime_now()
}

// --- Math and Random ---

#[no_mangle]
pub unsafe extern "C" fn vajra_math_sin(x: f64) -> f64 {
    let pi = 3.141592653589793;
    let mut z = x % (2.0 * pi);
    if z < -pi { z += 2.0 * pi; }
    if z > pi { z -= 2.0 * pi; }
    if z < -pi / 2.0 {
        z = -pi - z;
    } else if z > pi / 2.0 {
        z = pi - z;
    }
    let z2 = z * z;
    z * (1.0 - z2 * (1.0 / 6.0 - z2 * (1.0 / 120.0 - z2 * (1.0 / 5040.0 - z2 / 362880.0))))
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_cos(x: f64) -> f64 {
    let pi = 3.141592653589793;
    vajra_math_sin(x + pi / 2.0)
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_tan(x: f64) -> f64 {
    let s = vajra_math_sin(x);
    let c = vajra_math_cos(x);
    if c.abs() < 1e-15 { 0.0 } else { s / c }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_sqrt(x: f64) -> f64 {
    let ret: f64;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "sqrtsd {}, {}",
        out(xmm_reg) ret,
        in(xmm_reg) x
    );
    #[cfg(not(target_arch = "x86_64"))]
    { ret = x; }
    ret
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_abs(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

unsafe fn vajra_math_ln(x: f64) -> f64 {
    if x <= 0.0 { return -1e300; }
    let mut val = x;
    let mut k = 0.0;
    while val > 1.5 {
        val /= 2.718281828459045;
        k += 1.0;
    }
    while val < 0.5 {
        val *= 2.718281828459045;
        k -= 1.0;
    }
    let z = val - 1.0;
    let z2 = z * z;
    let z3 = z2 * z;
    let z4 = z3 * z;
    let z5 = z4 * z;
    let approx = z - z2/2.0 + z3/3.0 - z4/4.0 + z5/5.0;
    approx + k
}

unsafe fn vajra_math_exp(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for i in 1..20 {
        term *= x / i as f64;
        sum += term;
    }
    sum
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_log(x: f64) -> f64 {
    vajra_math_ln(x)
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_pow(x: f64, y: f64) -> f64 {
    let y_int = y as i32;
    if (y - y_int as f64).abs() < 1e-9 {
        let mut res = 1.0;
        let mut base = x;
        let mut exp = if y_int < 0 { -y_int } else { y_int };
        while exp > 0 {
            if exp & 1 == 1 {
                res *= base;
            }
            base *= base;
            exp >>= 1;
        }
        if y_int < 0 { 1.0 / res } else { res }
    } else {
        if x <= 0.0 { return 0.0; }
        let lnx = vajra_math_ln(x);
        vajra_math_exp(y * lnx)
    }
}

static mut G_RANDOM_STATE: u64 = 0x123456789abcdef;

#[no_mangle]
pub unsafe extern "C" fn vajra_random_int(min: i64, max: i64) -> i64 {
    let mut x = G_RANDOM_STATE;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    G_RANDOM_STATE = x;
    if min >= max { return min; }
    let range = (max - min) as u64;
    let val = x % range;
    min + val as i64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_random_float() -> f64 {
    let mut x = G_RANDOM_STATE;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    G_RANDOM_STATE = x;
    (x & 0xFFFFFFFFFFFF) as f64 / 281474976710655.0
}

#[no_mangle]
pub unsafe extern "C" fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return f64::NAN;
    }
    let quot = (x / y) as i64;
    x - (quot as f64) * y
}
