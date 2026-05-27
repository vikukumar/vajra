use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let win_dest = Path::new(&out_dir).join("runtime_win.o");
    let linux_dest = Path::new(&out_dir).join("runtime_linux.o");

    println!("cargo:rerun-if-changed=src/runtime/runtime.rs");
    println!("cargo:rerun-if-changed=src/runtime/runtime_common.rs");
    println!("cargo:rerun-if-changed=src/runtime/runtime_windows.rs");
    println!("cargo:rerun-if-changed=src/runtime/runtime_linux.rs");
    println!("cargo:rerun-if-changed=src/runtime/runtime_macos.rs");

    // 1. Compile Windows Runtime Object
    let win_status = Command::new("rustc")
        .args([
            "--crate-type=staticlib",
            "--emit=obj",
            "--target=x86_64-pc-windows-msvc",
            "-C",
            "panic=abort",
            "-C",
            "opt-level=3",
            "src/runtime/runtime.rs",
            "-o",
            win_dest.to_str().unwrap(),
        ])
        .status();

    match win_status {
        Ok(s) if s.success() => {}
        _ => {
            // Fallback: Copy checked-in runtime_win.o
            let fallback_win = Path::new("src/runtime/runtime_win.o");
            if fallback_win.exists() {
                std::fs::copy(fallback_win, &win_dest).unwrap_or(0);
            }
        }
    }

    // 2. Compile Linux Runtime Object
    let linux_status = Command::new("rustc")
        .args([
            "--crate-type=staticlib",
            "--emit=obj",
            "--target=x86_64-unknown-linux-gnu",
            "-C",
            "panic=abort",
            "-C",
            "opt-level=3",
            "src/runtime/runtime.rs",
            "-o",
            linux_dest.to_str().unwrap(),
        ])
        .status();

    match linux_status {
        Ok(s) if s.success() => {
            // Success! Save a copy as a checked-in fallback
            let fallback_linux = Path::new("src/runtime/runtime_linux.o");
            std::fs::copy(&linux_dest, fallback_linux).unwrap_or(0);
        }
        _ => {
            // Fallback: Copy checked-in runtime_linux.o
            let fallback_linux = Path::new("src/runtime/runtime_linux.o");
            if fallback_linux.exists() {
                std::fs::copy(fallback_linux, &linux_dest).unwrap_or(0);
            }
        }
    }

    // Support for the generic host/legacy runtime.o (backward compatibility)
    let dest_path = Path::new(&out_dir).join("runtime.o");
    #[cfg(target_os = "windows")]
    std::fs::copy(&win_dest, &dest_path).unwrap_or(0);
    #[cfg(not(target_os = "windows"))]
    std::fs::copy(&linux_dest, &dest_path).unwrap_or(0);
}
