#![deny(warnings)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::shadow_unrelated,
    clippy::similar_names,
    clippy::struct_excessive_bools
)]

use clap::{Parser as ClapParser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use inkwell::context::Context;
use vajra_core::lexer::Lexer;
use vajra_core::parser::Parser;
use vajra_core::codegen::Codegen;
use vajra_core::eval::Evaluator;


#[derive(ClapParser, Debug)]
#[command(author, version, about = "Vajra Compiler v0.1 - Powering the Intelligence of Tomorrow", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Compile a Vajra source file into a pure (.o) object file
    Compile {
        /// Source file to compile (.v, .vj, or .vajra)
        source: String,

        /// Output object binary name
        #[arg(short, long, default_value = "output.o")]
        output: String,

        /// Custom LLVM target triple for cross-compilation (e.g. aarch64-unknown-linux-gnu)
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Build a source file or an existing (.o) object file into a runnable native OS binary (.exe)
    Build {
        /// Input file (Sanskrit source file or a .o object file)
        input: String,

        /// Output executable binary name
        #[arg(short, long, default_value = "output.exe")]
        output: String,

        /// Custom LLVM target triple for cross-compilation (e.g. aarch64-unknown-linux-gnu)
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Compile and run a source file immediately in dev mode
    Run {
        /// Source file to run (.v, .vj, or .vajra)
        source: String,
    },
    /// Scan a directory and run all Vajra integration test cases
    Test {
        /// Directory containing test cases
        #[arg(short, long, default_value = "tests")]
        dir: String,
    },
    /// Start the interactive console (REPL) like python
    Repl,
}

fn merge_imports(statements: Vec<vajra_core::ast::Statement>, current_dir: &Path, parsed_files: &mut std::collections::HashSet<std::path::PathBuf>) -> anyhow::Result<Vec<vajra_core::ast::Statement>> {
    let mut merged = Vec::new();
    for stmt in statements {
        if let vajra_core::ast::Statement::Import(path_str) = stmt {
            let import_path = current_dir.join(&path_str);
            let canonical_path = fs::canonicalize(&import_path)
                .unwrap_or_else(|_| import_path.clone());
            
            if !parsed_files.contains(&canonical_path) {
                parsed_files.insert(canonical_path.clone());
                println!("Importing package: {}", import_path.display());
                if !import_path.exists() {
                    anyhow::bail!("Import Error: File not found: {}", import_path.display());
                }
                
                let input = fs::read_to_string(&import_path)?;
                let lexer = Lexer::new(&input);
                let mut parser = Parser::new(lexer);
                let program = parser.parse_program();
                
                let import_dir = import_path.parent().unwrap_or(Path::new("."));
                let imported_statements = merge_imports(program.statements, import_dir, parsed_files)?;
                merged.extend(imported_statements);
            }
        } else {
            merged.push(stmt);
        }
    }
    Ok(merged)
}

fn compile_source_to_obj(source: &str, output_o: &str, target: Option<&str>) -> anyhow::Result<()> {
    let path = Path::new(source);
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_str().unwrap_or("");
        if ext_str != "v" && ext_str != "vj" && ext_str != "vajra" {
            anyhow::bail!("Invalid file extension. Please provide a .v, .vj, or .vajra file.");
        }
    } else {
        anyhow::bail!("No file extension found. Please provide a .v, .vj, or .vajra file.");
    }
    
    // 1. Read source
    let input = fs::read_to_string(source)?;

    // 2. Lexical Analysis
    let lexer = Lexer::new(&input);

    // 3. Parsing
    let mut parser = Parser::new(lexer);
    let mut program = parser.parse_program();

    // 4. Resolve and Merge Imports
    let source_path = Path::new(source);
    let source_dir = source_path.parent().unwrap_or(Path::new("."));
    let mut parsed_files = std::collections::HashSet::new();
    if let Ok(canon) = fs::canonicalize(source_path) {
        parsed_files.insert(canon);
    }
    program.statements = merge_imports(program.statements, source_dir, &mut parsed_files)?;

    // 5. Codegen
    let context = Context::create();
    let codegen = Codegen::new(&context, "vajra_module");
    
    if let Err(e) = codegen.compile_program(&program) {
        anyhow::bail!("Compilation Error: {}", e);
    }

    // 5. Output Object File
    codegen.output_to_file(output_o, target).map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}

fn link_executable(obj_path: &str, out_exe_path: &str) -> anyhow::Result<()> {
    let temp_dir = std::env::temp_dir();
    let runtime_c_path = temp_dir.join("vajra_runtime.c");
    let runtime_obj_path = temp_dir.join("vajra_runtime.obj");

    let runtime_code = r#"
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <windows.h>
#endif

void* vajra_gc_alloc(size_t size) {
    return malloc(size);
}

void vajra_gc_free(void* ptr) {
    free(ptr);
}

void vajra_throw_exception(const char* msg) {
    fprintf(stderr, "क्रैश! अनपेक्षित अपवाद (Unhandled Exception): %s\n", msg);
    exit(1);
}

void __main(void) {
#ifdef _WIN32
    SetConsoleOutputCP(65001);
#endif
}

typedef struct {
    void (*worker_fn)(long long, long long, long long*, long long);
    long long start;
    long long end;
    long long* sum_ptr;
    long long nodes;
} ParallelTask;

#ifdef _WIN32
DWORD WINAPI parallel_thread_proc_helper(LPVOID param) {
    ParallelTask* task = (ParallelTask*)param;
    task->worker_fn(task->start, task->end, task->sum_ptr, task->nodes);
    return 0;
}
#endif

void vajra_parallel_for(
    long long start, 
    long long end, 
    void (*worker_fn)(long long, long long, long long*, long long), 
    long long* sum_ptr, 
    long long nodes
) {
#ifdef _WIN32
    SYSTEM_INFO sysinfo;
    GetSystemInfo(&sysinfo);
    int num_threads = sysinfo.dwNumberOfProcessors;
    if (num_threads < 1) num_threads = 1;
    if (num_threads > 64) num_threads = 64;

    long long total_iters = end - start;
    if (total_iters <= 0) return;

    if (total_iters < 100) {
        worker_fn(start, end, sum_ptr, nodes);
        return;
    }

    if (num_threads > total_iters) {
        num_threads = (int)total_iters;
    }

    HANDLE* threads = (HANDLE*)malloc(sizeof(HANDLE) * num_threads);
    ParallelTask* tasks = (ParallelTask*)malloc(sizeof(ParallelTask) * num_threads);

    long long chunk_size = total_iters / num_threads;
    long long rem = total_iters % num_threads;

    long long current_start = start;
    for (int i = 0; i < num_threads; i++) {
        long long current_end = current_start + chunk_size + (i < rem ? 1 : 0);
        
        tasks[i].worker_fn = worker_fn;
        tasks[i].start = current_start;
        tasks[i].end = current_end;
        tasks[i].sum_ptr = sum_ptr;
        tasks[i].nodes = nodes;

        threads[i] = CreateThread(NULL, 0, parallel_thread_proc_helper, &tasks[i], 0, NULL);
        current_start = current_end;
    }

    WaitForMultipleObjects(num_threads, threads, TRUE, INFINITE);

    for (int i = 0; i < num_threads; i++) {
        CloseHandle(threads[i]);
    }

    free(threads);
    free(tasks);
#else
    worker_fn(start, end, sum_ptr, nodes);
#endif
}
"#;
    fs::write(&runtime_c_path, runtime_code)?;

    let target = "x86_64-pc-windows-msvc";
    
    // Discover cl.exe to compile runtime.c
    let mut cl_tool = cc::windows_registry::find(target, "cl.exe")
        .ok_or_else(|| anyhow::anyhow!("Failed to find MSVC compiler cl.exe. Ensure Build Tools are installed."))?;

    println!("Compiling Vajra runtime using MSVC...");
    let compile_status = cl_tool
        .arg("/O2")
        .arg("/c")
        .arg(&runtime_c_path)
        .arg(format!("/Fo{}", runtime_obj_path.to_str().unwrap()))
        .status()?;

    if !compile_status.success() {
        anyhow::bail!("Failed to compile Vajra runtime with MSVC");
    }

    // Discover link.exe to link user obj and runtime obj
    let mut link_tool = cc::windows_registry::find(target, "link.exe")
        .ok_or_else(|| anyhow::anyhow!("Failed to find MSVC linker link.exe. Ensure Build Tools are installed."))?;

    println!("Linking executable with MSVC...");
    let link_status = link_tool
        .arg(obj_path)
        .arg(&runtime_obj_path)
        .arg(format!("/OUT:{}", out_exe_path))
        .arg("/SUBSYSTEM:CONSOLE")
        .status()?;

    if !link_status.success() {
        anyhow::bail!("Failed to link Vajra executable with MSVC");
    }

    // Clean up temporary compilation artifacts
    let _ = fs::remove_file(runtime_c_path);
    let _ = fs::remove_file(runtime_obj_path);

    Ok(())
}

fn run_file(source: &str) -> anyhow::Result<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        anyhow::bail!("Source file not found: {}", source);
    }
    
    let temp_exe = if cfg!(windows) {
        ".\\_temp_run_exec.exe"
    } else {
        "./_temp_run_exec"
    };

    println!("Compiling {} for execution...", source);
    
    let temp_o = "_temp_run_obj.o";
    compile_source_to_obj(source, temp_o, None)?;
    
    link_executable(temp_o, temp_exe)?;
    let _ = fs::remove_file(temp_o);

    println!("Executing {}...\n", temp_exe);
    
    let mut child = std::process::Command::new(temp_exe)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to run compiled executable: {}", e))?;
    
    let status = child.wait()?;
    
    let _ = fs::remove_file(temp_exe);
    
    if !status.success() {
        if let Some(code) = status.code() {
            anyhow::bail!("Program exited with non-zero status code: {}", code);
        } else {
            anyhow::bail!("Program terminated by signal");
        }
    }

    Ok(())
}

fn run_test_suite(test_dir: &str) -> anyhow::Result<()> {
    let path = Path::new(test_dir);
    if !path.exists() || !path.is_dir() {
        anyhow::bail!("Test directory not found: {}", test_dir);
    }

    println!("============================================================");
    println!("Vajra Test Harness: Running integration tests in '{}'", test_dir);
    println!("============================================================");

    let mut passed = 0;
    let mut failed = 0;
    let mut total = 0;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let test_file = entry.path();
        if test_file.is_file() {
            if let Some(ext) = test_file.extension() {
                let ext_str = ext.to_str().unwrap_or("");
                if ext_str == "vj" || ext_str == "vajra" {
                    total += 1;
                    let file_name = test_file.file_name().unwrap().to_str().unwrap();
                    print!("Running test {:<30} ... ", file_name);
                    std::io::stdout().flush()?;

                    match run_single_test(&test_file) {
                        Ok(()) => {
                            println!("[PASS]");
                            passed += 1;
                        }
                        Err(e) => {
                            println!("[FAIL]");
                            println!("  ↳ Error: {}", e);
                            failed += 1;
                        }
                    }
                }
            }
        }
    }

    println!("============================================================");
    println!("Test Summary: Total: {}, Passed: {}, Failed: {}", total, passed, failed);
    println!("============================================================");

    if failed > 0 {
        anyhow::bail!("Some integration tests failed!");
    }

    Ok(())
}

fn run_single_test(test_file: &Path) -> anyhow::Result<()> {
    let content = fs::read_to_string(test_file)?;
    let mut expected_output = Vec::new();
    let mut expected_exit_code = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# EXPECT:") {
            let expected = trimmed["# EXPECT:".len()..].trim().to_string();
            expected_output.push(expected);
        } else if trimmed.starts_with("# EXPECT_EXIT:") {
            if let Ok(code) = trimmed["# EXPECT_EXIT:".len()..].trim().parse::<i32>() {
                expected_exit_code = code;
            }
        }
    }

    let temp_o = "_temp_test_obj.o";
    let temp_exe = if cfg!(windows) {
        ".\\_temp_test_exec.exe"
    } else {
        "./_temp_test_exec"
    };

    let _ = fs::remove_file(temp_o);
    let _ = fs::remove_file(temp_exe);

    if let Err(e) = compile_source_to_obj(test_file.to_str().unwrap(), temp_o, None) {
        anyhow::bail!("Compilation failed: {}", e);
    }

    if let Err(e) = link_executable(temp_o, temp_exe) {
        let _ = fs::remove_file(temp_o);
        anyhow::bail!("Linking failed: {}", e);
    }
    let _ = fs::remove_file(temp_o);

    let output = std::process::Command::new(temp_exe)
        .output();

    let _ = fs::remove_file(temp_exe);

    let output = match output {
        Ok(out) => out,
        Err(e) => anyhow::bail!("Failed to execute compiled binary: {}", e),
    };

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code != expected_exit_code {
        anyhow::bail!("Exit code mismatch. Expected {}, got {}", expected_exit_code, exit_code);
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stdout_lines: Vec<&str> = stdout_str.lines().map(str::trim).collect();

    for (i, expected) in expected_output.iter().enumerate() {
        if i >= stdout_lines.len() {
            anyhow::bail!("Expected output line not found: '{}'", expected);
        }
        if stdout_lines[i] != expected {
            anyhow::bail!("Output mismatch at line {}. Expected '{}', got '{}'", i + 1, expected, stdout_lines[i]);
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    // 1. Intercept direct single file execution (Dev Mode auto build & run)
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() == 2 && !raw_args[1].starts_with('-') {
        let path = Path::new(&raw_args[1]);
        if path.exists() && path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_str().unwrap_or("");
                if ext_str == "vj" || ext_str == "vajra" {
                    println!("Vajra Single-File Dev Mode execution initiated for {}", raw_args[1]);
                    return run_file(&raw_args[1]);
                }
            }
        }
    }

    // 2. Parse standard Clap subcommands
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Compile { source, output, target }) => {
            println!("Vajra Compile Process Initiated");
            compile_source_to_obj(source, output, target.as_deref())?;
            println!("Successfully compiled {} to pure object file {}", source, output);
        }
        Some(Commands::Build { input, output, target }) => {
            println!("Vajra Build Process Initiated");
            
            let input_path = Path::new(input);
            let obj_file_to_link = if let Some(ext) = input_path.extension() {
                let ext_str = ext.to_str().unwrap_or("");
                if ext_str == "o" || ext_str == "obj" {
                    input.clone()
                } else if ext_str == "v" || ext_str == "vj" || ext_str == "vajra" {
                    let temp_o = format!("{}.o", input_path.file_stem().unwrap().to_str().unwrap());
                    println!("Compiling {} to intermediate object file {}...", input, temp_o);
                    compile_source_to_obj(input, &temp_o, target.as_deref())?;
                    temp_o
                } else {
                    anyhow::bail!("Unsupported file type. Please provide a source (.vajra, .v, .vj) or an object (.o, .obj) file.");
                }
            } else {
                anyhow::bail!("No file extension found.");
            };

            // Link dynamic object code with runtime stubs using MSVC link.exe
            link_executable(&obj_file_to_link, output)?;

            // Clean up intermediate object file if we compiled it on-the-fly
            if obj_file_to_link != *input {
                let _ = fs::remove_file(&obj_file_to_link);
            }

            println!("Successfully built executable binary: {}", output);
        }
        Some(Commands::Run { source }) => {
            run_file(source)?;
        }
        Some(Commands::Test { dir }) => {
            run_test_suite(dir)?;
        }
        Some(Commands::Repl) | None => {
            run_repl()?;
        }
    }

    Ok(())
}

fn run_repl() -> anyhow::Result<()> {
    println!("Vajra Interactive Console v0.1 - Powering the Intelligence of Tomorrow");
    println!("Type Sanskrit/Hindi code and press Enter. Type 'exit', 'प्रस्थान' or 'बाहर' to exit.");
    println!();

    let mut evaluator = Evaluator::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    loop {
        print!(">>> ");
        stdout.flush()?;

        let mut line = String::new();
        let bytes_read = stdin.read_line(&mut line)?;
        if bytes_read == 0 {
            println!();
            break;
        }

        let trimmed = line.trim();
        if trimmed == "exit" || trimmed == "प्रस्थान" || trimmed == "बाहर" {
            break;
        }

        if trimmed.is_empty() {
            continue;
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let lexer = Lexer::new(trimmed);
            let mut parser = Parser::new(lexer);
            let program = parser.parse_program();
            evaluator.eval_program(&program)
        }));

        match result {
            Ok(eval_res) => match eval_res {
                Ok(val) => {
                    if val != vajra_core::eval::Value::Void {
                        println!("{}", val);
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                }
            },
            Err(_) => {
                eprintln!("Error: Invalid syntax");
            }
        }
    }

    std::panic::set_hook(default_hook);

    Ok(())
}
