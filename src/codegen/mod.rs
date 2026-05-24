//! Vajra Code Generation — Module dispatcher
//! Routes AST compilation to the appropriate backend (x86_64, aarch64, wasm)

pub mod x86_64;
pub mod ast_to_ir;

use crate::ir::IrModule;
use anyhow::Result;

/// Available code generation backends
#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    X86_64,
    AArch64,
    Wasm32,
}

impl Backend {
    pub fn from_triple(triple: &str) -> Self {
        if triple.contains("aarch64") || triple.contains("arm64") {
            Backend::AArch64
        } else if triple.contains("wasm") {
            Backend::Wasm32
        } else {
            Backend::X86_64
        }
    }

    /// Returns the host backend for the current machine
    pub fn host() -> Self {
        #[cfg(target_arch = "x86_64")]
        return Backend::X86_64;
        #[cfg(target_arch = "aarch64")]
        return Backend::AArch64;
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        return Backend::X86_64; // default
    }
}

/// Compile an IR module to object file bytes
pub fn compile_to_object(module: &IrModule, backend: &Backend) -> Result<Vec<u8>> {
    match backend {
        Backend::X86_64 => x86_64::compile(module),
        Backend::AArch64 => {
            anyhow::bail!("AArch64 backend not yet implemented in v0.1 — planned for v0.1")
        }
        Backend::Wasm32 => {
            anyhow::bail!("WASM backend not yet implemented in v0.1 — planned for v0.1")
        }
    }
}
