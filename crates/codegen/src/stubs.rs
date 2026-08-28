use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;

pub struct StubGenerator;

impl StubGenerator {
    pub fn generate(_schema: &Schema) -> Result<GeneratedCode> {
        let code = Self::generate_readme();

        Ok(GeneratedCode {
            code,
            description: "Stubs README".to_string(),
        })
    }

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
