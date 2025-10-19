pub mod api_codegen; // Sprint 9: API code generation
pub mod ast;
pub mod codegen;
pub mod component_stubs; // Sprint 17: Component stub generation
pub mod lexer;
pub mod openapi_codegen;
pub mod parser;
pub mod route_handlers; // Sprint 17: API route handler generation
pub mod typescript_codegen; // Sprint 10: TypeScript SDK generation
pub mod typescript_component_props; // Sprint 17: Component props generation

// Re-export from new locations for backward compatibility
pub use codegen::api::ApiCodeGenerator;
pub use codegen::openapi::OpenApiGenerator;

pub use ast::Schema;
pub use codegen::CodeGenerator;
pub use component_stubs::{ComponentStubGenerator, StubTemplate};
pub use parser::Parser;
pub use route_handlers::RouteHandlerGenerator;
pub use typescript_codegen::TypeScriptGenerator;
pub use typescript_component_props::ComponentPropsGenerator;
