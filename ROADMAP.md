# Vajra 1.0 Readiness Checklist

## Required Before 1.0

- Implement a Vajra-written compiler core and bootstrap it with the current Rust-hosted compiler.
- Add conformance tests for lexer, parser, type checking, IR lowering, codegen, linker, runtime, OOP, BigInt, callbacks, loops, errors, and imports.
- Replace runtime stubs with real implementations or remove the advertised feature.
- Finish Linux runtime parity and decide whether macOS is supported in 1.0 or deferred.
- Implement production-grade closure/function type semantics, including captured variables.
- Complete compiled try/catch behavior or mark exceptions as interpreter-only until a later release.
- Harden garbage collection with deterministic stress tests, root validation, allocation metrics, and memory safety checks.
- Remove user-facing panics and unwrap-driven crashes from compiler paths.
- Freeze syntax and standard library APIs for a compatibility window.
- Publish honest benchmark scripts with reproducible inputs, outputs, and machine details.

## Current Bootstrap Strategy

1. Keep the Rust-hosted compiler as stage0.
2. Write a minimal Vajra parser and IR emitter in Vajra.
3. Compile the Vajra compiler source with stage0 into stage1.
4. Use stage1 to compile the same Vajra compiler source into stage2.
5. Compare stage1 and stage2 behavior over the conformance suite.

The project should not be called self-hosted until stage1 can build stage2 successfully.
