const EOL: &[u8] = b"\n";

#[cfg(target_arch = "x86_64")]
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

#[cfg(not(target_arch = "x86_64"))]
unsafe fn sys_write(_fd: i32, _buf: *const u8, _count: usize) -> isize {
    0
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(not(target_arch = "x86_64"))]
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

#[cfg(target_arch = "x86_64")]
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

#[cfg(not(target_arch = "x86_64"))]
unsafe fn sys_open(_path: *const u8, _flags: i32, _mode: i32) -> isize {
    -1
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(not(target_arch = "x86_64"))]
unsafe fn sys_read(_fd: isize, _buf: *mut u8, _count: usize) -> isize {
    0
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(not(target_arch = "x86_64"))]
unsafe fn sys_close(_fd: isize) -> isize {
    0
}

#[cfg(target_arch = "x86_64")]
unsafe fn sys_exit(code: i32) -> ! {
    core::arch::asm!(
        "syscall",
        in("rax") 60, // SYS_exit
        in("rdi") code,
        options(noreturn)
    );
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn sys_exit(_code: i32) -> ! {
    loop {}
}

fn allocation_failed(ptr: *mut c_void) -> bool {
    ptr.is_null() || ptr as isize == -1
}

unsafe fn get_thread_id() -> u64 {
    1
}

unsafe fn get_stack_base() -> *mut u8 {
    core::ptr::null_mut()
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
    }
    0
}

unsafe fn print_raw(buf: *const u8, len: usize) {
    sys_write(1, buf, len);
}

unsafe fn read_raw(buf: *mut u8, len: usize) -> usize {
    let read = sys_read(0, buf, len);
    if read < 0 { 0 } else { read as usize }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_exit(code: i64) -> ! {
    sys_exit(code as i32);
}

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

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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
