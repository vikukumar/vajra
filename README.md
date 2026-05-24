# Vajra Programming Language

Vajra is an experimental systems language and compiler project. The compiler is currently Rust-hosted, with a custom x86-64 code generator and built-in PE/ELF linking so Vajra applications do not require LLVM, GCC, Clang, MSVC, or an external linker.

## Current Status

Vajra is not ready for a 1.0 self-hosted release yet. The project can compile and run selected Vajra programs on x86-64, but the compiler itself is not yet written in Vajra.

Working today:

- Multilingual keywords and aliases for common syntax.
- Brace-style and indentation-style source preprocessing.
- Native x86-64 code generation.
- Windows PE output and Linux ELF output.
- Tagged small integers with BigInt fallback and runtime guardrails.
- Basic classes, methods, dispatch, sockets, printing, raw allocation, loops, nested functions, and simple callback function values.
- Selected loop-folding optimizations for known math kernels.

Still in progress:

- True self-hosting: rewriting the compiler core in Vajra and bootstrapping it.
- Production-grade type checking and diagnostics.
- Capturing closures and first-class function types.
- Complete compiled exception handling.
- Full Linux/macOS runtime parity.
- Production-grade garbage collection and memory validation.
- Broad automated conformance tests.

## Build

```bash
cargo build --release
```

The compiler binary is written to `target/release/vajrac.exe` on Windows or `target/release/vajrac` on Unix.

## Compile A Vajra Program

```bash
vajrac compile tests/expression_grouping.vj -o expression_grouping.exe
```

## Run Tests

```bash
cargo test
```

The test suite includes CLI integration tests that compile and run Vajra fixture programs.

## Versioning

The repository is still pre-1.0. A real `1.0.0` release should only happen after the self-hosting path, runtime, diagnostics, tests, and target support are stable enough to preserve compatibility.

See [ROADMAP.md](ROADMAP.md) for the 1.0 readiness checklist.
