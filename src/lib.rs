pub mod ast;
pub mod lexer;
pub mod parser;
pub mod codegen;
pub mod api_codegen; // Sprint 9: API code generation
pub mod typescript_codegen; // Sprint 10: TypeScript SDK generation
pub mod openapi_codegen; // Sprint 13: OpenAPI/Swagger documentation

#[cfg(test)]
mod edge_case_tests;

pub use parser::Parser;
pub use codegen::CodeGenerator;
pub use api_codegen::ApiCodeGenerator;
pub use typescript_codegen::TypeScriptGenerator;
pub use openapi_codegen::OpenApiGenerator;
pub use ast::Schema;
