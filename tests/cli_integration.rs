use std::path::{Path, PathBuf};
use std::io::Write;
use std::process::{Command, Stdio};

fn vajrac() -> &'static str {
    env!("CARGO_BIN_EXE_vajrac")
}

fn temp_exe(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let suffix = std::process::id();
    if cfg!(windows) {
        path.push(format!("{name}_{suffix}.exe"));
    } else {
        path.push(format!("{name}_{suffix}"));
    }
    path
}

fn compile_fixture(source: &str, output: &Path) {
    let status = Command::new(vajrac())
        .args(["compile", source, "-o"])
        .arg(output)
        .status()
        .expect("failed to spawn vajrac");
    assert!(status.success(), "vajrac failed compiling {source}");
}

fn run_exe(path: &Path) -> String {
    let output = Command::new(path)
        .output()
        .expect("failed to run compiled executable");
    assert!(output.status.success(), "compiled program failed");
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn run_exe_with_stdin(path: &Path, stdin: &str) -> String {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run compiled executable");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe missing")
        .write_all(stdin.as_bytes())
        .expect("failed to write stdin");
    let output = child
        .wait_with_output()
        .expect("failed to read compiled executable output");
    assert!(output.status.success(), "compiled program failed");
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn expression_grouping_compiles_and_runs() {
    let exe = temp_exe("vajra_expression_grouping");
    compile_fixture("tests/expression_grouping.vj", &exe);
    let stdout = run_exe(&exe);
    let _ = std::fs::remove_file(exe);
    assert_eq!(stdout, "24\n24\n24\n");
}

#[test]
fn nested_callbacks_compile_and_run() {
    let exe = temp_exe("vajra_nested_callbacks");
    compile_fixture("tests/nested_callbacks.vj", &exe);
    let stdout = run_exe(&exe);
    let _ = std::fs::remove_file(exe);
    assert_eq!(stdout, "42\n");
}

#[test]
fn mixed_bigint_loop_does_not_exhaust_heap() {
    let exe = temp_exe("vajra_mixed_bigint_cmp");
    compile_fixture("tests/mixed_bigint_cmp_loop.vj", &exe);
    let stdout = run_exe(&exe);
    let _ = std::fs::remove_file(exe);
    assert_eq!(stdout, "0\n1\n2\n3\n");
}

#[test]
fn compiled_readline_reads_stdin() {
    let exe = temp_exe("vajra_readline_compiled");
    compile_fixture("tests/readline_compiled.vj", &exe);
    let stdout = run_exe_with_stdin(&exe, "Vajra\n");
    let _ = std::fs::remove_file(exe);
    assert_eq!(stdout, "Vajra\n");
}
