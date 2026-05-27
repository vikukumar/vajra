const EOL: &[u8] = b"\r\n";

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

static mut G_STDOUT: *mut c_void = core::ptr::null_mut();
static mut G_STDIN: *mut c_void = core::ptr::null_mut();

fn allocation_failed(ptr: *mut c_void) -> bool {
    ptr.is_null()
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

#[no_mangle]
pub unsafe extern "C" fn platform_alloc(size: usize) -> *mut u8 {
    let ptr = VirtualAlloc(
        core::ptr::null_mut(),
        size,
        0x3000, // MEM_COMMIT | MEM_RESERVE
        0x04,   // PAGE_READWRITE
    );
    if ptr.is_null() {
        core::ptr::null_mut()
    } else {
        ptr as *mut u8
    }
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

#[no_mangle]
pub unsafe extern "C" fn vajra_exit(code: i64) -> ! {
    ExitProcess(code as u32);
}

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

extern "system" {
    fn LoadLibraryA(lpLibFileName: *const u8) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

static mut G_WINSOCK_INITIALIZED: bool = false;
static mut G_WSA_STARTUP: Option<unsafe extern "system" fn(u16, *mut u8) -> i32> = None;
static mut G_SOCKET: Option<unsafe extern "system" fn(i32, i32, i32) -> usize> = None;
static mut G_CONNECT: Option<unsafe extern "system" fn(usize, *const u8, i32) -> i32> = None;
static mut G_SEND: Option<unsafe extern "system" fn(usize, *const u8, i32, i32) -> i32> = None;
static mut G_RECV: Option<unsafe extern "system" fn(usize, *mut u8, i32, i32) -> i32> = None;
static mut G_CLOSESOCKET: Option<unsafe extern "system" fn(usize) -> i32> = None;

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

static mut G_GET_SYSTEM_TIME_AS_FILE_TIME: Option<unsafe extern "system" fn(*mut u64)> = None;

unsafe fn ensure_datetime() {
    if G_GET_SYSTEM_TIME_AS_FILE_TIME.is_some() { return; }
    let k32 = LoadLibraryA(b"kernel32.dll\0".as_ptr());
    G_GET_SYSTEM_TIME_AS_FILE_TIME = core::mem::transmute(GetProcAddress(k32, b"GetSystemTimeAsFileTime\0".as_ptr()));
}
