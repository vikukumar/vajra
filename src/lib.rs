/// Vajra Compiler — Public Library API
/// No external compiler dependency (LLVM, GCC, MSVC, Clang) — 100% self-hosted.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod ir;
pub mod codegen;
pub mod linker;
pub mod runtime;
pub mod eval;
pub mod lint;

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
    // 1. Lex + Parse
    let lexer = Lexer::new(source);
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
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);
    parser.parse_program()
}

/// Lower AST to IR only
pub fn lower_to_ir(source: &str, module_name: &str) -> Result<IrModule> {
    let program = parse_source(source);
    codegen::ast_to_ir::lower(&program, module_name)
}
