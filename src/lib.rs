//! Vajra Compiler — Public Library API
//! Rust-hosted Vajra compiler core with no LLVM/GCC/MSVC/Clang dependency for Vajra program builds.

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
    clippy::for_kv_map,
    clippy::empty_line_after_doc_comments,
    clippy::chars_next_cmp,
    clippy::manual_map
)]

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod ir;
pub mod codegen;
pub mod linker;
pub mod runtime;
pub mod eval;
pub mod lint;
pub mod preprocessor;

pub use ast::Program;
pub use lexer::Lexer;
pub use parser::Parser;
pub use ir::IrModule;
pub use codegen::Backend;
pub use linker::TargetPlatform;

use anyhow::Result;

/// Full compilation pipeline: source → executable bytes
pub fn compile_source(
    source: &str,
    module_name: &str,
    backend: Backend,
    platform: TargetPlatform,
) -> Result<Vec<u8>> {
    // 1. Preprocess + Lex + Parse
    let preprocessed = preprocessor::preprocess_source(source);
    let lexer = Lexer::new(&preprocessed);
    let mut parser = Parser::new(lexer);
    let program = parser.parse_program();

    // 2. AST → IR
    let ir_module = codegen::ast_to_ir::lower(&program, module_name)?;

    // 3. IR → Object file bytes
    let obj_bytes = codegen::compile_to_object(&ir_module, &backend)?;

    // 4. Get embedded runtime object
    let runtime_bytes = runtime::get_runtime_object_bytes(&platform);

    // 5. Link → executable
    linker::link(&obj_bytes, &runtime_bytes, &platform, "main")
}

/// Parse only — returns the AST
pub fn parse_source(source: &str) -> Program {
    let preprocessed = preprocessor::preprocess_source(source);
    let lexer = Lexer::new(&preprocessed);
    let mut parser = Parser::new(lexer);
    parser.parse_program()
}

/// Lower AST to IR only
pub fn lower_to_ir(source: &str, module_name: &str) -> Result<IrModule> {
    let program = parse_source(source);
    codegen::ast_to_ir::lower(&program, module_name)
}
