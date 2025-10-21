pub mod ast;
pub mod lexer;
pub mod parser;

// Re-export main types for convenience
pub use ast::{
    ComponentProtocol, ComponentReference, Constraint, ConstraintParam, Field, FieldType, Model,
    RelationType, Schema, Struct,
};
pub use lexer::{Lexer, Token};
pub use parser::Parser;
