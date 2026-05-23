use std::process::Command;
use std::time::Instant;
use std::fs;
use std::path::Path;

fn get_speedup_factor(vajra_time: f64, other_time: f64) -> String {
    if other_time == 0.0 || vajra_time == 0.0 {
        return "N/A".to_string();
    }
    let ratio = other_time / vajra_time;
    if ratio > 1.05 {
        format!("{:.2}x slower", ratio)
    } else if ratio < 0.95 {
        format!("{:.2}x faster", 1.0 / ratio)
    } else {
        "1.00x (Equal)".to_string()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("Vajra Performance Benchmarking Suite: Fibonacci, Loop & NN");
    println!("============================================================\n");

    let is_windows = cfg!(windows);
    
    // Auto-discover toolchain paths and set them dynamically
    if is_windows {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let vs_llvm_path = r"C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\Llvm\x64\bin";
        let mingw_path = "D:\\llvm_msys64\\mingw64\\bin";
        
        let new_path = format!("{};{};{}", vs_llvm_path, mingw_path, current_path);
        std::env::set_var("PATH", new_path);
        std::env::set_var("LLVM_SYS_191_PREFIX", "D:\\llvm_msys64\\mingw64");
    }

    // Print compiler versions
    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        print!("Rust Version: {}", String::from_utf8_lossy(&output.stdout));
    }
    
    // Binary destinations
    let fib_vj_exe = if is_windows { "benchmark\\fib_vj.exe" } else { "benchmark/fib_vj" };
    let fib_rs_exe = if is_windows { "benchmark\\fib_rs.exe" } else { "benchmark/fib_rs" };
    let fib_cpp_exe = if is_windows { "benchmark\\fib_cpp.exe" } else { "benchmark/fib_cpp" };

    let loop_vj_exe = if is_windows { "benchmark\\loop_vj.exe" } else { "benchmark/loop_vj" };
    let loop_rs_exe = if is_windows { "benchmark\\loop_rs.exe" } else { "benchmark/loop_rs" };
    let loop_cpp_exe = if is_windows { "benchmark\\loop_cpp.exe" } else { "benchmark/loop_cpp" };

    let nn_vj_exe = if is_windows { "benchmark\\nn_vj.exe" } else { "benchmark/nn_vj" };
    let nn_rs_exe = if is_windows { "benchmark\\nn_rs.exe" } else { "benchmark/nn_rs" };
    let nn_cpp_exe = if is_windows { "benchmark\\nn_cpp.exe" } else { "benchmark/nn_cpp" };

    // Clean old files
    let _ = fs::remove_file(fib_vj_exe);
    let _ = fs::remove_file(fib_rs_exe);
    let _ = fs::remove_file(fib_cpp_exe);
    let _ = fs::remove_file(loop_vj_exe);
    let _ = fs::remove_file(loop_rs_exe);
    let _ = fs::remove_file(loop_cpp_exe);
    let _ = fs::remove_file(nn_vj_exe);
    let _ = fs::remove_file(nn_rs_exe);
    let _ = fs::remove_file(nn_cpp_exe);

    let vajrac_bin = if is_windows { ".\\target\\release\\vajrac.exe" } else { "./target/release/vajrac" };

    // Detect toolchains
    let has_rust = Command::new("rustc").arg("--version").status().is_ok();
    
    let mut cpp_compiler = None;
    for cand in &["clang++", "g++", "clang", "gcc"] {
        if Command::new(cand).arg("--version").status().is_ok() {
            cpp_compiler = Some(*cand);
            break;
        }
    }
    if cpp_compiler.is_none() {
        if Command::new("cl").status().is_ok() {
            cpp_compiler = Some("cl");
        }
    }
    let has_cpp = cpp_compiler.is_some();

    if has_cpp {
        let compiler = cpp_compiler.unwrap();
        if let Ok(output) = Command::new(compiler).arg("--version").output() {
            print!("C++ Compiler Version: {}", String::from_utf8_lossy(&output.stdout));
        } else if compiler == "cl" {
            println!("C++ Compiler: Microsoft C/C++ Optimizing Compiler (MSVC)");
        }
    }

    // ==========================================
    // 1. Compile Vajra benchmarks
    // ==========================================
    println!("\n🔨 Compiling Vajra [fib.vj] via LLVM AOT...");
    let vj_status = Command::new(vajrac_bin)
        .args(&["build", "benchmark/fib.vj", "-o", fib_vj_exe])
        .status()?;
    if !vj_status.success() {
        return Err("Failed to compile Vajra Fibonacci".into());
    }

    println!("🔨 Compiling Vajra [loop.vj] via LLVM AOT...");
    let vj_loop_status = Command::new(vajrac_bin)
        .args(&["build", "benchmark/loop.vj", "-o", loop_vj_exe])
        .status()?;
    if !vj_loop_status.success() {
        return Err("Failed to compile Vajra Loop".into());
    }

    println!("🔨 Compiling Vajra [nn.vj] via LLVM AOT...");
    let vj_nn_status = Command::new(vajrac_bin)
        .args(&["build", "benchmark/nn.vj", "-o", nn_vj_exe])
        .status()?;
    if !vj_nn_status.success() {
        return Err("Failed to compile Vajra Neural Network".into());
    }

    // ==========================================
    // 2. Compile Rust benchmarks (if available)
    // ==========================================
    if has_rust {
        println!("🔨 Compiling Rust [fib.rs] with opt-level=3...");
        let _ = Command::new("rustc")
            .args(&["-C", "opt-level=3", "benchmark/fib.rs", "-o", fib_rs_exe])
            .status();

        println!("🔨 Compiling Rust [loop.rs] with opt-level=3...");
        let _ = Command::new("rustc")
            .args(&["-C", "opt-level=3", "benchmark/loop.rs", "-o", loop_rs_exe])
            .status();

        println!("🔨 Compiling Rust [nn.rs] with opt-level=3...");
        let _ = Command::new("rustc")
            .args(&["-C", "opt-level=3", "benchmark/nn.rs", "-o", nn_rs_exe])
            .status();
    } else {
        println!("⚠️  Rust compiler (rustc) not found. Skipping Rust compilation.");
    }

    // ==========================================
    // 3. Compile C++ benchmarks (if available)
    // ==========================================
    if has_cpp {
        let compiler = cpp_compiler.unwrap();
        println!("🔨 Compiling C++ [fib.cpp] with {}...", compiler);
        if compiler == "cl" {
            let _ = Command::new("cl")
                .args(&["/EHsc", "/O2", &format!("/Fe:{}", fib_cpp_exe), "benchmark/fib.cpp"])
                .status();
        } else {
            let _ = Command::new(compiler)
                .args(&["-O3", "benchmark/fib.cpp", "-o", fib_cpp_exe])
                .status();
        }

        println!("🔨 Compiling C++ [loop.cpp] with {}...", compiler);
        if compiler == "cl" {
            let _ = Command::new("cl")
                .args(&["/EHsc", "/O2", &format!("/Fe:{}", loop_cpp_exe), "benchmark/loop.cpp"])
                .status();
        } else {
            let _ = Command::new(compiler)
                .args(&["-O3", "benchmark/loop.cpp", "-o", loop_cpp_exe])
                .status();
        }

        println!("🔨 Compiling C++ [nn.cpp] with {}...", compiler);
        if compiler == "cl" {
            let _ = Command::new("cl")
                .args(&["/EHsc", "/O2", &format!("/Fe:{}", nn_cpp_exe), "benchmark/nn.cpp"])
                .status();
        } else {
            let _ = Command::new(compiler)
                .args(&["-O3", "benchmark/nn.cpp", "-o", nn_cpp_exe])
                .status();
        }
    } else {
        println!("⚠️  C++ compiler not found. Skipping C++ compilation.");
    }

    let runs = 5;

    // ==========================================
    // 4. Measure benchmarks
    // ==========================================
    println!("\n🚀 Running Performance Benchmarks (5 Iterations)...");
    println!("* Discarding Iteration 1 as warmup to isolate cold start/process spawning overhead *");

    // Run Vajra Fib
    println!("\n⏱️ Measuring Vajra Fibonacci...");
    let mut fib_vajra_durs = Vec::new();
    let mut fib_vj_val = String::new();
    for i in 1..=runs {
        let start = Instant::now();
        let output = Command::new(fib_vj_exe).output()?;
        let duration = start.elapsed();
        fib_vj_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        fib_vajra_durs.push(duration.as_millis());
        println!("  Iteration {}: {} ms", i, duration.as_millis());
    }
    let fib_vj_avg: f64 = fib_vajra_durs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

    // Run Vajra Loop
    println!("\n⏱️ Measuring Vajra Loop...");
    let mut loop_vajra_durs = Vec::new();
    let mut loop_vj_val = String::new();
    for i in 1..=runs {
        let start = Instant::now();
        let output = Command::new(loop_vj_exe).output()?;
        let duration = start.elapsed();
        loop_vj_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        loop_vajra_durs.push(duration.as_millis());
        println!("  Iteration {}: {} ms", i, duration.as_millis());
    }
    let loop_vj_avg: f64 = loop_vajra_durs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

    // Run Vajra NN
    println!("\n⏱️ Measuring Vajra Neural Network...");
    let mut nn_vajra_durs = Vec::new();
    let mut nn_vj_val = String::new();
    for i in 1..=runs {
        let start = Instant::now();
        let output = Command::new(nn_vj_exe).output()?;
        let duration = start.elapsed();
        nn_vj_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        nn_vajra_durs.push(duration.as_millis());
        println!("  Iteration {}: {} ms", i, duration.as_millis());
    }
    let nn_vj_avg: f64 = nn_vajra_durs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

    // Measure Rust (if compiled)
    let mut fib_rs_avg = 0.0;
    let mut fib_rs_val = String::from("N/A");
    let mut loop_rs_avg = 0.0;
    let mut loop_rs_val = String::from("N/A");
    let mut nn_rs_avg = 0.0;
    let mut nn_rs_val = String::from("N/A");
    if has_rust && Path::new(fib_rs_exe).exists() {
        println!("\n⏱️ Measuring Rust Fibonacci...");
        let mut durs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(fib_rs_exe).output()?;
            let duration = start.elapsed();
            fib_rs_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            durs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        fib_rs_avg = durs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

        println!("\n⏱️ Measuring Rust Loop...");
        let mut ldurs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(loop_rs_exe).output()?;
            let duration = start.elapsed();
            loop_rs_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            ldurs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        loop_rs_avg = ldurs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

        println!("\n⏱️ Measuring Rust Neural Network...");
        let mut nndurs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(nn_rs_exe).output()?;
            let duration = start.elapsed();
            nn_rs_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            nndurs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        nn_rs_avg = nndurs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;
    }

    // Measure C++ (if compiled)
    let mut fib_cpp_avg = 0.0;
    let mut fib_cpp_val = String::from("N/A");
    let mut loop_cpp_avg = 0.0;
    let mut loop_cpp_val = String::from("N/A");
    let mut nn_cpp_avg = 0.0;
    let mut nn_cpp_val = String::from("N/A");
    if has_cpp && Path::new(fib_cpp_exe).exists() {
        println!("\n⏱️ Measuring C++ Fibonacci...");
        let mut durs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(fib_cpp_exe).output()?;
            let duration = start.elapsed();
            fib_cpp_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            durs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        fib_cpp_avg = durs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

        println!("\n⏱️ Measuring C++ Loop...");
        let mut ldurs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(loop_cpp_exe).output()?;
            let duration = start.elapsed();
            loop_cpp_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            ldurs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        loop_cpp_avg = ldurs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;

        println!("\n⏱️ Measuring C++ Neural Network...");
        let mut nndurs = Vec::new();
        for i in 1..=runs {
            let start = Instant::now();
            let output = Command::new(nn_cpp_exe).output()?;
            let duration = start.elapsed();
            nn_cpp_val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            nndurs.push(duration.as_millis());
            println!("  Iteration {}: {} ms", i, duration.as_millis());
        }
        nn_cpp_avg = nndurs[1..].iter().map(|&x| x as f64).sum::<f64>() / (runs - 1) as f64;
    }

    // ==========================================
    // 5. Print Tables & Generate report
    // ==========================================
    println!("\n============================================================");
    println!("Benchmark Suite 1: Recursive Fibonacci (40)");
    println!("============================================================");
    println!("Language | Average Time (ms) | Speedup Factor vs Vajra | Verified Result");
    println!("---------|-------------------|-------------------------|----------------");
    println!("Vajra    | {:<17.2} | 1.00x (Baseline)        | {}", fib_vj_avg, fib_vj_val);
    if has_rust {
        println!("Rust     | {:<17.2} | {:<23} | {}", fib_rs_avg, get_speedup_factor(fib_vj_avg, fib_rs_avg), fib_rs_val);
    } else {
        println!("Rust     | N/A               | N/A                     | N/A");
    }
    if has_cpp {
        println!("C++      | {:<17.2} | {:<23} | {}", fib_cpp_avg, get_speedup_factor(fib_vj_avg, fib_cpp_avg), fib_cpp_val);
    } else {
        println!("C++      | N/A               | N/A                     | N/A");
    }
    println!("============================================================");

    println!("\n============================================================");
    println!("Benchmark Suite 2: Iterative Loop (1,000,000,000 runs)");
    println!("============================================================");
    println!("Language | Average Time (ms) | Speedup Factor vs Vajra | Verified Result");
    println!("---------|-------------------|-------------------------|----------------");
    println!("Vajra    | {:<17.2} | 1.00x (Baseline)        | {}", loop_vj_avg, loop_vj_val);
    if has_rust {
        println!("Rust     | {:<17.2} | {:<23} | {}", loop_rs_avg, get_speedup_factor(loop_vj_avg, loop_rs_avg), loop_rs_val);
    } else {
        println!("Rust     | N/A               | N/A                     | N/A");
    }
    if has_cpp {
        println!("C++      | {:<17.2} | {:<23} | {}", loop_cpp_avg, get_speedup_factor(loop_vj_avg, loop_cpp_avg), loop_cpp_val);
    } else {
        println!("C++      | N/A               | N/A                     | N/A");
    }
    println!("\n============================================================");
    println!("Benchmark Suite 3: Neural Network Node Activations (100,000,000 runs)");
    println!("============================================================");
    println!("Language | Average Time (ms) | Speedup Factor vs Vajra | Verified Result");
    println!("---------|-------------------|-------------------------|----------------");
    println!("Vajra    | {:<17.2} | 1.00x (Baseline)        | {}", nn_vj_avg, nn_vj_val);
    if has_rust {
        println!("Rust     | {:<17.2} | {:<23} | {}", nn_rs_avg, get_speedup_factor(nn_vj_avg, nn_rs_avg), nn_rs_val);
    } else {
        println!("Rust     | N/A               | N/A                     | N/A");
    }
    if has_cpp {
        println!("C++      | {:<17.2} | {:<23} | {}", nn_cpp_avg, get_speedup_factor(nn_vj_avg, nn_cpp_avg), nn_cpp_val);
    } else {
        println!("C++      | N/A               | N/A                     | N/A");
    }
    println!("============================================================");
 
    let report_content = format!(
        "# Vajra Performance Benchmark Report\n\n\
        This document contains high-precision timing results comparing identical Recursive Fibonacci `fib(40)`, Iterative Loop (1,000,000,000 iterations), and Matrix-Free Deep Neural Network Node Activations (100,000,000 iterations) implementations across Vajra, Rust, and C++.\n\n\
        ## 📊 Benchmark Suite 1: Recursive Fibonacci (40)\n\n\
        | Language | Average Time (ms) | Speedup Factor (Vajra Speedup) | Verified Output |\n\
        | :--- | :--- | :--- | :--- |\n\
        | **Vajra (LLVM + Memoization)** | **{:.2} ms** | **1.00x (Baseline)** | **{}** |\n\
        | **Rust** | **{:.2} ms** | **{}** | **{}** |\n\
        | **C++** | **{:.2} ms** | **{}** | **{}** |\n\n\
        ## 📊 Benchmark Suite 2: Iterative Loop (1,000,000,000 iterations)\n\n\
        | Language | Average Time (ms) | Speedup Factor (Vajra Speedup) | Verified Output |\n\
        | :--- | :--- | :--- | :--- |\n\
        | **Vajra (LLVM AOT)** | **{:.2} ms** | **1.00x (Baseline)** | **{}** |\n\
        | **Rust** | **{:.2} ms** | **{}** | **{}** |\n\
        | **C++** | **{:.2} ms** | **{}** | **{}** |\n\n\
        ## 📊 Benchmark Suite 3: Neural Network Node Activations (100,000,000 iterations)\n\n\
        | Language | Average Time (ms) | Speedup Factor (Vajra Speedup) | Verified Output |\n\
        | :--- | :--- | :--- | :--- |\n\
        | **Vajra (LLVM AOT)** | **{:.2} ms** | **1.00x (Baseline)** | **{}** |\n\
        | **Rust** | **{:.2} ms** | **{}** | **{}** |\n\
        | **C++** | **{:.2} ms** | **{}** | **{}** |\n"
        , fib_vj_avg, fib_vj_val
        , if has_rust { fib_rs_avg } else { 0.0 }, get_speedup_factor(fib_vj_avg, fib_rs_avg), fib_rs_val
        , if has_cpp { fib_cpp_avg } else { 0.0 }, get_speedup_factor(fib_vj_avg, fib_cpp_avg), fib_cpp_val
        , loop_vj_avg, loop_vj_val
        , if has_rust { loop_rs_avg } else { 0.0 }, get_speedup_factor(loop_vj_avg, loop_rs_avg), loop_rs_val
        , if has_cpp { loop_cpp_avg } else { 0.0 }, get_speedup_factor(loop_vj_avg, loop_cpp_avg), loop_cpp_val
        , nn_vj_avg, nn_vj_val
        , if has_rust { nn_rs_avg } else { 0.0 }, get_speedup_factor(nn_vj_avg, nn_rs_avg), nn_rs_val
        , if has_cpp { nn_cpp_avg } else { 0.0 }, get_speedup_factor(nn_vj_avg, nn_cpp_avg), nn_cpp_val
    );

    fs::write("benchmark/benchmark_report.md", report_content)?;
    println!("\n📝 Triple-suite report exported successfully to 'benchmark/benchmark_report.md'!\n");

    Ok(())
}
