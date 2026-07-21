//! Component and computed field stub generator

use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;

/// Stub generator for components and computed fields
pub struct StubGenerator;

impl StubGenerator {
    /// Generate stub files for computed fields and components
    ///
    /// # Arguments
    ///
    /// * `_schema` - Parsed schema AST (currently unused but reserved for future use)
    ///
    /// # Returns
    ///
    /// Generated stub README content
    pub fn generate(_schema: &Schema) -> Result<GeneratedCode> {
        let code = Self::generate_readme();

        Ok(GeneratedCode {
            code,
            description: "Stubs README".to_string(),
        })
    }

    /// Generate README content for stubs directory
    fn generate_readme() -> String {
        r#"# Generated Stubs

This directory contains stub files for:

- Computed field implementations
- UI component implementations

These files are created once and should be edited by you to implement the actual logic.
They will not be overwritten by future code generation runs.
"#
        .to_string()
    }
}
