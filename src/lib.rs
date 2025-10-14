pub mod ast;
pub mod lexer;
pub mod parser;
pub mod codegen;
pub mod api_codegen; // Sprint 9: API code generation

#[cfg(test)]
mod edge_case_tests;

pub use parser::Parser;
pub use codegen::CodeGenerator;
pub use api_codegen::ApiCodeGenerator;
pub use ast::Schema;
