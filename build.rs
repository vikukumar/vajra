use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("runtime_win.o");

    println!("cargo:rerun-if-changed=src/runtime/runtime_win.rs");

    let status = Command::new("rustc")
        .args(&[
            "--crate-type=staticlib",
            "--emit=obj",
            "-C", "panic=abort",
            "-C", "opt-level=3",
            "src/runtime/runtime_win.rs",
            "-o",
            dest_path.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            // Fallback: if compilation fails or rustc is not ready, copy the checked-in runtime_win.o
            let fallback_path = Path::new("src/runtime/runtime_win.o");
            if fallback_path.exists() {
                std::fs::copy(fallback_path, &dest_path).unwrap();
            } else {
                panic!("Failed to compile runtime_win.rs and no fallback runtime_win.o found");
            }
        }
    }
}
