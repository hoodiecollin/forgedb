use crate::ast::{ManyToManyRelation, Schema};
use super::super::relations::junction::JunctionTableGenerator;

pub struct MultiFileGenerator;

impl MultiFileGenerator {
    pub fn new() -> Self {
        MultiFileGenerator
    }

    pub fn generate_common_imports(&self, schema: &Schema) -> (String, bool) {
        let common_imports = "use std::collections::HashMap;\nuse std::time::{SystemTime, UNIX_EPOCH};\nuse uuid::Uuid;\n";

        // Check if any model uses constraints - if so, add regex import
        let has_constraints = schema
            .models
            .iter()
            .any(|m| m.fields.iter().any(|f| !f.constraints.is_empty()));

        (common_imports.to_string(), has_constraints)
    }

    pub fn generate_mod_file(&self, schema: &Schema, m2m_relations: &[ManyToManyRelation]) -> String {
        let mut mod_content = String::new();
        mod_content.push_str("// Generated code - do not edit manually\n\n");

        // Export structs module if it exists
        if !schema.structs.is_empty() {
            mod_content.push_str("pub mod structs;\n");
            mod_content.push_str("pub use structs::*;\n\n");
        }

        for model in &schema.models {
            mod_content.push_str(&format!("mod {}_storage;\n", model.name.to_lowercase()));
            mod_content.push_str(&format!(
                "pub use {}_storage::*;\n\n",
                model.name.to_lowercase()
            ));
        }

        for m2m in m2m_relations {
            let junction_name = JunctionTableGenerator::junction_table_name(m2m);
            mod_content.push_str(&format!("mod {}_junction;\n", junction_name.to_lowercase()));
            mod_content.push_str(&format!(
                "pub use {}_junction::*;\n\n",
                junction_name.to_lowercase()
            ));
        }

        mod_content
    }
}
