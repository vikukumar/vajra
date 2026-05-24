//! Vajra Compiler Driver (vajrac)
//! Rust-hosted Vajra compiler driver.
//! Supports: compile, run, build, test — all without LLVM, GCC, MSVC, Clang

#![allow(
    clippy::needless_borrows_for_generic_args,
    clippy::new_without_default,
    clippy::collapsible_match,
    clippy::single_match,
    clippy::get_first,
    clippy::missing_const_for_thread_local,
    clippy::collapsible_if,
    clippy::match_like_matches_macro,
    clippy::manual_range_contains,
    clippy::unnecessary_sort_by,
    clippy::len_zero,
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::manual_strip,
    clippy::implicit_saturating_sub,
    clippy::for_kv_map
)]

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use vajra_core::{
    ast::{Program, Statement},
    codegen::{self, ast_to_ir, Backend},
    eval,
    ir::IrModule,
    lexer::Lexer,
    linker::{self, TargetPlatform},
    parser::Parser,
    runtime,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(ClapParser, Debug)]
#[command(
    name = "vajrac",
    version,
    about = "Vajra: a Rust-hosted programming language compiler with multi-human-language support\nNo LLVM, GCC, Clang, MSVC, or external linker is required for Vajra program builds.",
    long_about = "Vajra is currently a Rust-hosted programming language featuring multi-human-language translation (English, Hindi, Sanskrit, Spanish, Chinese, Arabic, and more) at the keyword level.\n\nIt features a custom native code generator for x86-64 and a built-in linker emitting COFF/PE executables on Windows and ELF executables on Linux. No external compiler toolchains like LLVM, GCC, MSVC, Clang, or ld are required to compile, link, and run Vajra programs.\n\nUSAGE EXAMPLES:\n  Start REPL:          vajrac\n  Interpret a file:    vajrac exec hello.vj\n  Build project:       vajrac build\n  Compile file:        vajrac compile main.vj -o main.exe --target x86_64-pc-windows-msvc"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Compile a Vajra source file to a native executable
    ///
    /// Compile a Vajra source file (.vj or .vajra) directly to a standalone native binary.
    ///
    /// EXAMPLES:
    ///   vajrac compile main.vj -o main.exe
    ///   vajrac compile main.vj --target x86_64-unknown-linux-gnu -o main
    ///   vajrac compile main.vj --emit-ir
    ///
    /// TARGETS:
    ///   x86_64-pc-windows-msvc    - Windows PE32+ executable
    ///   x86_64-unknown-linux-gnu  - Linux ELF64 executable
    ///   x86_64-apple-darwin       - macOS target (Linux-compatible ELF fallback)
    Compile {
        /// Source file (.vajra or .vj)
        file: String,
        /// Output executable path (defaults to file stem)
        #[arg(short, long, default_value = "")]
        output: String,
        /// Target triple (e.g. x86_64-pc-windows-msvc, x86_64-unknown-linux-gnu)
        #[arg(long, default_value = "")]
        target: String,
        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "0")]
        opt: u8,
        /// Emit intermediate representation (IR) instead of native code
        #[arg(long)]
        emit_ir: bool,
        /// Emit object file only (do not link into executable)
        #[arg(long)]
        emit_obj: bool,
    },
    /// Build a Vajra project (reads project.vajra or main.vajra)
    ///
    /// Compile and link a Vajra project based on a main entry file.
    /// Resolves transitive imports concurrently and compiles them into a single binary.
    ///
    /// EXAMPLES:
    ///   vajrac build
    ///   vajrac build main.vj -o my_app.exe --release
    Build {
        /// Optional source file (defaults to main.vajra)
        #[arg(default_value = "main.vajra")]
        file: String,
        /// Output executable path (defaults to file stem)
        #[arg(short, long, default_value = "")]
        output: String,
        /// Release mode (opt-level 3 optimized build)
        #[arg(long)]
        release: bool,
    },
    /// Compile and run a Vajra source file immediately
    ///
    /// Compile the file to a temporary executable, run it, and clean up the binary afterward.
    /// Arguments to the executable can be passed after '--'.
    ///
    /// EXAMPLES:
    ///   vajrac run hello.vj
    ///   vajrac run calc.vj -- 10 20
    Run {
        /// Source file to run
        file: String,
        /// Arguments to pass to the compiled program
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Execute a Vajra file using the built-in interpreter (no compilation)
    ///
    /// Runs the interpreter directly on the AST without compiling or linking,
    /// ideal for quick scripting, testing, or environments without assembly codegen support.
    ///
    /// EXAMPLES:
    ///   vajrac exec hello.vj
    ///   vajrac exec calc.vj -- 10 20
    Exec {
        /// Source file to interpret
        file: String,
        /// Arguments to pass to the interpreter
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Start the interactive Vajra REPL shell
    ///
    /// Starts the Read-Eval-Print Loop. Supports typing line-by-line Vajra statements.
    /// Special command ':help' prints REPL commands and multi-language keywords.
    /// Special command ':ir <code>' displays IR for a snippet.
    ///
    /// EXAMPLES:
    ///   vajrac repl
    Repl,
    /// Check a Vajra file for errors (no output)
    ///
    /// Parses the file and lowers it to IR to verify syntax correctness,
    /// resolving all imports transitively without outputting any code or binaries.
    ///
    /// EXAMPLES:
    ///   vajrac check hello.vj
    Check {
        /// Source file to check
        file: String,
    },
    /// Lint a Vajra file for style, unused variables, and potential issues
    ///
    /// Runs a set of lint rules to check for style conventions, unused variables,
    /// or logic traps.
    ///
    /// EXAMPLES:
    ///   vajrac lint hello.vj
    Lint {
        /// Source file to lint
        file: String,
    },
    /// Show the AST for a source file (debugging tool)
    ///
    /// Prints a formatted JSON/debug representation of the parsed Abstract Syntax Tree.
    ///
    /// EXAMPLES:
    ///   vajrac ast hello.vj
    Ast {
        /// Source file
        file: String,
    },
    /// Show the IR for a source file (debugging tool)
    ///
    /// Prints a formatted representation of the target-independent Vajra IR.
    ///
    /// EXAMPLES:
    ///   vajrac ir hello.vj
    Ir {
        /// Source file
        file: String,
    },
    /// Print version and capability information
    ///
    /// Displays version details, targets, human languages supported, and compiler architecture constraints.
    ///
    /// EXAMPLES:
    ///   vajrac info
    Info,
}

fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "unexpected compiler failure"
        };
        if let Some(location) = info.location() {
            eprintln!(
                "Compiler internal error at {}:{}: {}",
                location.file(),
                location.line(),
                message
            );
        } else {
            eprintln!("Compiler internal error: {}", message);
        }
    }));

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Compile {
            file,
            output,
            target,
            opt,
            emit_ir,
            emit_obj,
        }) => cmd_compile(&file, &output, &target, opt, emit_ir, emit_obj),
        Some(Command::Build {
            file,
            output,
            release,
        }) => cmd_build(&file, &output, release),
        Some(Command::Run { file, args }) => cmd_run(&file, &args),
        Some(Command::Exec { file, args }) => cmd_exec(&file, &args),
        Some(Command::Repl) => cmd_repl(),
        Some(Command::Check { file }) => cmd_check(&file),
        Some(Command::Lint { file }) => cmd_lint(&file),
        Some(Command::Ast { file }) => cmd_ast(&file),
        Some(Command::Ir { file }) => cmd_ir(&file),
        Some(Command::Info) => {
            cmd_info();
            Ok(())
        }
        None => cmd_repl(),
    }
}

// ─── Command Implementations ────────────────────────────────────────────────

fn cmd_compile(
    file: &str,
    output: &str,
    target: &str,
    _opt_level: u8,
    emit_ir: bool,
    emit_obj: bool,
) -> Result<()> {
    eprintln!("🔰 Vajra v{} — Compiling '{}' ...", VERSION, file);

    // 1. Concurrent Compile and Merge
    let compiled_files = Arc::new(Mutex::new(HashSet::new()));
    let canonical_entry = Path::new(file)
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize entry path: '{}'", file))?
        .to_string_lossy()
        .to_string();
    compiled_files.lock().unwrap().insert(canonical_entry);

    let ir_module = compile_module_transitively(file, compiled_files)?;
    eprintln!(
        "   ✓ IR generated ({} functions)",
        ir_module.functions.len()
    );

    if emit_ir {
        print_ir(&ir_module);
        return Ok(());
    }

    // 3. Determine target
    let (backend, platform) = resolve_target(target);
    eprintln!("   ✓ Target: {:?} on {:?}", backend, platform);

     // 4. Code generation → object file
     let obj_bytes = codegen::compile_to_object(&ir_module, &backend, &platform)
         .with_context(|| "Code generation failed")?;
    eprintln!("   ✓ Object generated ({} bytes)", obj_bytes.len());

    if emit_obj {
        let obj_path = output_path(file, output, "o", &platform);
        fs::write(&obj_path, &obj_bytes).with_context(|| format!("Cannot write '{}'", obj_path))?;
        eprintln!("   ✓ Object written to '{}'", obj_path);
        return Ok(());
    }

    // 5. Link with embedded runtime
    let runtime_bytes = runtime::get_runtime_object_bytes(&platform);
    let exe_bytes = linker::link(&obj_bytes, &runtime_bytes, &platform, "main")
        .with_context(|| "Linking failed")?;
    eprintln!("   ✓ Linked ({} bytes)", exe_bytes.len());

    // 6. Write executable
    let exe_path = output_path(file, output, platform.exe_extension(), &platform);
    fs::write(&exe_path, &exe_bytes).with_context(|| format!("Cannot write '{}'", exe_path))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&exe_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exe_path, perms)?;
    }

    eprintln!("   ✓ Executable: '{}'", exe_path);
    eprintln!("\n✅ Build complete!");
    Ok(())
}

fn cmd_build(file: &str, output: &str, release: bool) -> Result<()> {
    let opt = if release { 3 } else { 0 };
    eprintln!("🔨 Building '{}' (release={})", file, release);
    cmd_compile(file, output, "", opt, false, false)
}

fn cmd_run(file: &str, extra_args: &[String]) -> Result<()> {
    // 1. Compile to temp directory
    let tmp_dir = std::env::temp_dir();
    let module_name = Path::new(file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let platform = TargetPlatform::host();
    let ext = platform.exe_extension();
    let exe_name = if ext.is_empty() {
        format!("{}/{}", tmp_dir.display(), module_name)
    } else {
        format!("{}/{}.{}", tmp_dir.display(), module_name, ext)
    };

    cmd_compile(file, &exe_name, "", 0, false, false)?;

    // 2. Execute
    eprintln!("\n🚀 Running '{}' ...\n", exe_name);
    let status = std::process::Command::new(&exe_name)
        .args(extra_args)
        .status();

    // Clean up temporary executable
    let _ = std::fs::remove_file(&exe_name);

    let status = status.with_context(|| format!("Failed to execute '{}'", exe_name))?;
    std::process::exit(status.code().unwrap_or(0));
}

fn cmd_lint(file: &str) -> Result<()> {
    let source = fs::read_to_string(file).with_context(|| format!("Cannot read '{}'", file))?;
    let program = parse(&source)?;
    let mut linter = vajra_core::lint::Linter::new();
    linter.lint_program(&program);
    if linter.warnings.is_empty() {
        println!("✨ No lint warnings found in '{}'.", file);
        Ok(())
    } else {
        println!(
            "⚠️ Found {} lint warnings in '{}':",
            linter.warnings.len(),
            file
        );
        for warning in &linter.warnings {
            println!("  {}", warning);
        }
        anyhow::bail!("Linting failed with {} warnings", linter.warnings.len())
    }
}

fn cmd_exec(file: &str, _extra_args: &[String]) -> Result<()> {
    // Use the interpreter (eval.rs) — no compilation
    let source = fs::read_to_string(file).with_context(|| format!("Cannot read '{}'", file))?;

    let program = parse(&source)?;
    eprintln!("🔰 Vajra v{} — Executing '{}' (interpreter mode)", VERSION, file);

    let mut interpreter = eval::Interpreter::new();
    if let Err(e) = interpreter.run(&program) {
        anyhow::bail!("Runtime error: {}", e);
    }
    Ok(())
}

fn cmd_repl() -> Result<()> {
    use std::io::{self, BufRead, Write};

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              Vajra Language — Interactive REPL               ║");
    println!("║  Version {:<7} — Rust-hosted | No External Toolchain Needed ║", VERSION);
    println!("║  Languages: English, हिंदी, संस्कृत, தமிழ், العربية, 中文   ║");
    println!("║  Type 'exit' or 'quit' to leave | ':compile <code>' to emit  ║");
    println!("╚══════════════════════════════════════════════════════════════╝");

    let mut interpreter = eval::Interpreter::new();
    let stdin = io::stdin();
    let mut buffer = String::new();
    let mut brace_depth: i32 = 0;

    loop {
        if brace_depth == 0 {
            print!("vajra> ");
        } else {
            print!("    ...  ");
        }
        io::stdout().flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim();

        match trimmed {
            "exit"
            | "quit"
            | "निर्गम"
            | "بيرون"
            | "退出"
            | "निकलने"
            | "बहिर्गमन"
            | "इग्ज़िट"
            | "نکلने"
            | "nikalo"
            | "nikal jao" => {
                println!("\n🙏 नमस्ते | Vande Mataram | Jai Hind | Bharat Mata ki Jai");
                break;
            }
            _ if trimmed.starts_with(":ir ") => {
                let code = &trimmed[4..];
                match vajra_core::lower_to_ir(code, "repl") {
                    Ok(module) => print_ir(&module),
                    Err(e) => eprintln!("IR Error: {}", e),
                }
                continue;
            }
            _ if trimmed.starts_with(":compile ") => {
                let code = &trimmed[9..];
                eprintln!("(compile-mode: use 'vajrac compile <file>' for full compilation)");
                match vajra_core::lower_to_ir(code, "repl") {
                    Ok(module) => {
                        print_ir(&module);
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
                continue;
            }
            ":help" => {
                println!("REPL Commands:");
                println!("  :ir <code>        — Show IR for a snippet");
                println!("  :compile <code>   — Show IR output");
                println!("  exit / quit       — Exit the REPL");
                println!("\nVajra supports these languages:");
                println!("  English: fn main() {{ let x = 42; print(x) }}");
                println!("  हिंदी:   कार्य मुख्य() {{ मान x = 42; लिखो(x) }}");
                println!("  தமிழ்:   செயல்பாடு முக்கிய() {{ மாறி x = 42 }}");
                println!("  中文:    函数 主函数() {{ 变量 x = 42; 打印(x) }}");
                println!("  Español: función principal() {{ variable x = 42 }}");
                continue;
            }
            _ => {}
        }

        // Count braces for multi-line input
        for c in trimmed.chars() {
            match c {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }
        buffer.push_str(&line);

        if brace_depth <= 0 {
            brace_depth = 0;
            let code = buffer.trim().to_string();
            buffer.clear();

            if code.is_empty() {
                continue;
            }

            let program = match parse(&code) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    continue;
                }
            };
            if let Err(e) = interpreter.run(&program) {
                eprintln!("❌ Runtime error: {}", e);
            }
        }
    }
    Ok(())
}

fn cmd_check(file: &str) -> Result<()> {
    let compiled_files = Arc::new(Mutex::new(HashSet::new()));
    let canonical_entry = Path::new(file)
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize entry path: '{}'", file))?
        .to_string_lossy()
        .to_string();
    compiled_files.lock().unwrap().insert(canonical_entry);

    let _ir_module =
        compile_module_transitively(file, compiled_files).with_context(|| "IR lowering error")?;

    eprintln!("✅ '{}' is valid Vajra code", file);
    Ok(())
}

fn cmd_ast(file: &str) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let program = parse(&source)?;
    println!("{:#?}", program);
    Ok(())
}

fn cmd_ir(file: &str) -> Result<()> {
    let compiled_files = Arc::new(Mutex::new(HashSet::new()));
    let canonical_entry = Path::new(file)
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize entry path: '{}'", file))?
        .to_string_lossy()
        .to_string();
    compiled_files.lock().unwrap().insert(canonical_entry);

    let ir = compile_module_transitively(file, compiled_files)?;
    print_ir(&ir);
    Ok(())
}

fn cmd_info() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                    Vajra Language v{:<7}                     ║", VERSION);
    println!("║        Rust-hosted — Zero External Vajra Build Toolchain          ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Backend          : Native x86-64 (pure Rust)                    ║");
    println!("║  Object Format    : COFF (Windows) / ELF (Linux)                 ║");
    println!("║  Linker           : Own PE32+ / ELF64 linker (pure Rust)         ║");
    println!("║  Runtime          : Own runtime (kernel32 / Linux syscalls only)  ║");
    println!("║  LLVM Dependency  : NONE                                          ║");
    println!("║  GCC Dependency   : NONE                                          ║");
    println!("║  MSVC Dependency  : NONE                                          ║");
    println!("║  Clang Dependency : NONE                                          ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Human Languages Supported (keywords):                            ║");
    println!("║    English, Sanskrit (संस्कृत), Hindi (हिंदी), Tamil (தமிழ்)      ║");
    println!("║    Arabic (العربية), Chinese (中文), Spanish (Español)             ║");
    println!("║    Marathi (मराठी), Bengali (বাংলা), Telugu (తెలుగు)               ║");
    println!("║    Kannada (ಕನ್ನಡ), Gujarati (ગુજરાતી), Russian (Русский)          ║");
    println!("║    French (Français), German (Deutsch), Japanese (日本語)          ║");
    println!("║    Korean (한국어)                                                  ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Targets:                                                          ║");
    println!("║    x86_64-pc-windows — PE32+ EXE                                  ║");
    println!("║    x86_64-linux-gnu  — ELF64                                       ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
}

fn get_imports(program: &Program) -> Vec<String> {
    let mut imports = Vec::new();
    for stmt in &program.statements {
        if let Statement::Import(path) = stmt {
            imports.push(path.clone());
        }
    }
    imports
}

fn resolve_import_path(current_file: &str, import_name: &str) -> Result<String> {
    let current_dir = Path::new(current_file).parent().unwrap_or(Path::new(""));
    let mut target_path = current_dir.join(import_name);
    if !target_path.exists() {
        let name_str = import_name.to_string();
        if !name_str.ends_with(".vj") && !name_str.ends_with(".vajra") {
            let try_vj = current_dir.join(format!("{}.vj", name_str));
            if try_vj.exists() {
                target_path = try_vj;
            } else {
                let try_vajra = current_dir.join(format!("{}.vajra", name_str));
                if try_vajra.exists() {
                    target_path = try_vajra;
                }
            }
        }
    }
    let canonical = target_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize import path: {:?}", target_path))?;
    Ok(canonical.to_string_lossy().to_string())
}

fn compile_module_transitively(
    file_path: &str,
    compiled_files: Arc<Mutex<HashSet<String>>>,
) -> Result<IrModule> {
    let source =
        fs::read_to_string(file_path).with_context(|| format!("Cannot read '{}'", file_path))?;
    let module_name = Path::new(file_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let program = parse(&source)?;
    let imports = get_imports(&program);

    let mut main_ir = ast_to_ir::lower(&program, &module_name)
        .with_context(|| format!("AST→IR lowering failed for {}", file_path))?;

    let mut handles = Vec::new();
    for imp in imports {
        let resolved = resolve_import_path(file_path, &imp)?;
        let mut set = compiled_files.lock().unwrap();
        if !set.contains(&resolved) {
            set.insert(resolved.clone());
            drop(set);

            let compiled_files_clone = Arc::clone(&compiled_files);
            let handle =
                thread::spawn(move || compile_module_transitively(&resolved, compiled_files_clone));
            handles.push(handle);
        }
    }

    for handle in handles {
        let imported_ir = handle
            .join()
            .map_err(|e| anyhow::anyhow!("Compilation thread panicked: {:?}", e))??;
        main_ir.merge(imported_ir);
    }

    Ok(main_ir)
}

fn parse(source: &str) -> Result<Program> {
    let preprocessed = vajra_core::preprocessor::preprocess_source(source);
    let lexer = Lexer::new(&preprocessed);
    let mut parser = Parser::new(lexer);
    Ok(parser.parse_program())
}

fn resolve_target(triple: &str) -> (Backend, TargetPlatform) {
    if triple.is_empty() {
        (Backend::host(), TargetPlatform::host())
    } else {
        (
            Backend::from_triple(triple),
            TargetPlatform::from_triple(triple),
        )
    }
}

fn output_path(source_file: &str, output: &str, ext: &str, _platform: &TargetPlatform) -> String {
    if !output.is_empty() {
        return output.to_string();
    }
    let stem = Path::new(source_file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{}.{}", stem, ext)
    }
}

fn print_ir(module: &vajra_core::ir::IrModule) {
    // use vajra_core::ir::*;
    println!("=== IR Module: {} ===", module.name);
    println!("Globals:");
    for g in &module.globals {
        println!("  @{} = {:?}", g.name, String::from_utf8_lossy(&g.data));
    }
    println!("\nFunctions:");
    for func in &module.functions {
        if func.is_extern {
            println!(
                "  extern fn {}({})",
                func.name,
                func.params
                    .iter()
                    .map(|p| format!("%{}", p.val))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            continue;
        }
        let main_mark = if func.is_main { " [main]" } else { "" };
        println!(
            "\n  fn {}({}){}:",
            func.name,
            func.params
                .iter()
                .map(|p| format!("{}: {:?}", p.name, p.ty))
                .collect::<Vec<_>>()
                .join(", "),
            main_mark
        );
        for block in &func.blocks {
            println!("    .{}:", block.label);
            for instr in &block.instrs {
                println!("      {:?}", instr);
            }
            if let Some(term) = &block.terminator {
                println!("      [TERM] {:?}", term);
            }
        }
    }
}
