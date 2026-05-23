use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("runtime.o");

    println!("cargo:rerun-if-changed=src/runtime/runtime.rs");

    let status = Command::new("rustc")
        .args(&[
            "--crate-type=staticlib",
            "--emit=obj",
            "-C", "panic=abort",
            "-C", "opt-level=3",
            "src/runtime/runtime.rs",
            "-o",
            dest_path.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            // Fallback: if compilation fails or rustc is not ready, copy the checked-in runtime.o or runtime_win.o
            let fallback_path = Path::new("src/runtime/runtime.o");
            if fallback_path.exists() {
                std::fs::copy(fallback_path, &dest_path).unwrap();
            } else {
                let fallback_win = Path::new("src/runtime/runtime_win.o");
                if fallback_win.exists() {
                    std::fs::copy(fallback_win, &dest_path).unwrap();
                } else {
                    panic!("Failed to compile runtime.rs and no fallback runtime.o or runtime_win.o found");
                }
            }
        }
    }
}
