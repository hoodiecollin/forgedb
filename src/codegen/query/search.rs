use crate::ast::{FieldType, Model};

pub struct SearchGenerator;

impl SearchGenerator {
    pub fn new() -> Self {
        SearchGenerator
    }

    pub fn generate_search_methods(&self, code: &mut String, model: &Model) {
        // Generate search methods for full-text indexed fields (Sprint 18)
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        for field in &model.fields {
            if field.fulltext_indexed {
                // Only generate if we have a UUID ID field
                if let Some(id_field) = id_field {
                    if matches!(id_field.field_type, FieldType::Uuid) {
                        // Generate search method
                        code.push_str(&format!(
                            "    /// Full-text search on the '{}' field\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn search_{}(&self, query: &str) -> Vec<{}> {{\n",
                            field.name, model.name
                        ));
                        code.push_str(&format!("        let matches = self.{}_fulltext.read().unwrap().search(query);\n", field.name));
                        code.push_str("        let mut results = Vec::new();\n");
                        code.push_str("        for doc_match in matches {\n");
                        code.push_str(
                            "            if let Some(record) = self.records.iter().enumerate()\n",
                        );
                        code.push_str(&format!("                .find(|(i, r)| !self.tombstones[*i] && r.{} == doc_match.doc_id)\n", id_field.name));
                        code.push_str("                .map(|(_, r)| r.clone()) {\n");
                        code.push_str("                results.push(record);\n");
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                        code.push_str("        results\n");
                        code.push_str("    }\n\n");

                        // Generate phrase search method
                        code.push_str(&format!(
                            "    /// Phrase search on the '{}' field\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn search_{}_phrase(&self, phrase: &str) -> Vec<{}> {{\n",
                            field.name, model.name
                        ));
                        code.push_str(&format!("        let matches = self.{}_fulltext.read().unwrap().search_phrase(phrase);\n", field.name));
                        code.push_str("        let mut results = Vec::new();\n");
                        code.push_str("        for doc_match in matches {\n");
                        code.push_str(
                            "            if let Some(record) = self.records.iter().enumerate()\n",
                        );
                        code.push_str(&format!("                .find(|(i, r)| !self.tombstones[*i] && r.{} == doc_match.doc_id)\n", id_field.name));
                        code.push_str("                .map(|(_, r)| r.clone()) {\n");
                        code.push_str("                results.push(record);\n");
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                        code.push_str("        results\n");
                        code.push_str("    }\n\n");
                    }
                }
            }
        }
    }
}
