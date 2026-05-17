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
    clippy::struct_excessive_bools,
    clippy::uninlined_format_args,
    clippy::manual_let_else,
    clippy::bool_to_int_with_if,
    clippy::match_like_matches_macro,
    clippy::single_match,
    clippy::items_after_statements,
    clippy::ptr_arg,
    clippy::redundant_closure_for_method_calls,
    clippy::derive_partial_eq_without_eq,
    clippy::ignored_unit_patterns,
    clippy::collapsible_else_if,
    clippy::redundant_pattern_matching,
    clippy::match_single_binding,
    clippy::manual_range_contains,
    clippy::needless_return,
    clippy::match_bool,
    clippy::collapsible_match,
    clippy::single_match_else,
    clippy::useless_format,
    clippy::manual_string_new,
    clippy::new_without_default,
    clippy::match_same_arms,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    clippy::cognitive_complexity,
    clippy::non_std_lazy_statics,
    clippy::unnecessary_wraps,
    clippy::manual_assert,
    clippy::assigning_clones,
    clippy::cloned_instead_of_copied,
    clippy::redundant_else,
    clippy::manual_strip
)]

pub mod lexer;
pub mod ast;
pub mod parser;
pub mod codegen;
pub mod eval;

