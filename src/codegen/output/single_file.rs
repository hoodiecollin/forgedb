use crate::ast::Schema;

pub struct SingleFileGenerator;

impl SingleFileGenerator {
    pub fn new() -> Self {
        SingleFileGenerator
    }

    pub fn generate_header(&self, schema: &Schema) -> String {
        let mut code = String::new();

        // Add standard imports
        code.push_str("// Generated code - do not edit manually\n\n");
        code.push_str("use std::collections::HashMap;\n");
        code.push_str("use std::time::{SystemTime, UNIX_EPOCH};\n");
        code.push_str("use uuid::Uuid;\n");

        // Check if any model uses constraints - if so, add regex import
        let has_constraints = schema
            .models
            .iter()
            .any(|m| m.fields.iter().any(|f| !f.constraints.is_empty()));

        if has_constraints {
            code.push_str("use regex;\n");
        }

        code.push_str("\n");

        code
    }
}
