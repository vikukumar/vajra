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

pub mod lexer;
pub mod ast;
pub mod parser;
pub mod codegen;
pub mod eval;

