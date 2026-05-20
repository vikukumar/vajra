/// Vajra Runtime I/O — OS-level input/output without C stdlib
/// Windows: kernel32.dll WriteFile/ReadFile
/// Linux: direct syscall(SYS_write, SYS_read)

/// High-level print functions implemented at the Rust level for the REPL.
/// For compiled programs, the x86-64 machine code versions in mod.rs are used.

pub fn print_str(s: &str) {
    #[cfg(target_os = "windows")]
    {
       // use std::os::windows::io::AsRawHandle;
        use std::io::Write;
        let _ = std::io::stdout().write_all(s.as_bytes());
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::io::Write;
        let _ = std::io::stdout().write_all(s.as_bytes());
    }
}

pub fn print_i64(n: i64) {
    use std::io::Write;
    let s = format!("{}\n", n);
    let _ = std::io::stdout().write_all(s.as_bytes());
}

pub fn print_f64(f: f64) {
    use std::io::Write;
    let s = format!("{}\n", f);
    let _ = std::io::stdout().write_all(s.as_bytes());
}

pub fn read_line() -> String {
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
    buf.trim_end_matches('\n').trim_end_matches('\r').to_string()
}
