use crate::ast::Model;
use super::super::utils::{is_virtual_field, get_field_param_name, get_field_param_type};

pub struct BatchGenerator;

impl BatchGenerator {
    pub fn new() -> Self {
        BatchGenerator
    }

    /// Generate batch operation methods (Sprint 19)
    pub fn generate_batch_methods(&self, code: &mut String, model: &Model) {
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            // Generate batch insert
            code.push_str("    /// Batch insert multiple records\n");

            // Count non-auto-generated fields
            let non_auto_fields: Vec<_> = model.fields.iter()
                .filter(|f| !is_virtual_field(f) && !f.auto_generate)
                .collect();

            let use_tuple = non_auto_fields.len() > 1;

            if use_tuple {
                code.push_str("    pub fn insert_batch(&mut self, records: Vec<(");
            } else {
                code.push_str("    pub fn insert_batch(&mut self, records: Vec<");
            }

            let mut first = true;
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }
                if !field.auto_generate {
                    if !first && use_tuple {
                        code.push_str(", ");
                    }
                    code.push_str(&get_field_param_type(field));
                    first = false;
                }
            }

            if use_tuple {
                code.push_str(")");
            }
            code.push_str(&format!(">) -> Result<Vec<{}>, String> {{\n", model.name));
            code.push_str("        let mut results = Vec::new();\n");
            code.push_str("        for record in records {\n");

            if use_tuple {
                code.push_str("            // Unpack tuple\n");
            }

            let mut tuple_index = 0;
            let mut param_names = Vec::new();
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }
                if !field.auto_generate {
                    let param_name = get_field_param_name(field);
                    param_names.push(param_name.clone());
                    if use_tuple {
                        code.push_str(&format!(
                            "            let {} = record.{};\n",
                            param_name, tuple_index
                        ));
                    } else {
                        // For single field, record IS the value
                        code.push_str(&format!("            let {} = record;\n", param_name));
                    }
                    tuple_index += 1;
                }
            }

            code.push_str("            let result = self.insert(");
            code.push_str(&param_names.join(", "));
            code.push_str(")?;\n");
            code.push_str("            results.push(result);\n");
            code.push_str("        }\n");
            code.push_str("        Ok(results)\n");
            code.push_str("    }\n\n");

            // Generate batch delete
            code.push_str("    /// Batch delete multiple records\n");
            code.push_str(&format!(
                "    pub fn delete_batch(&mut self, ids: Vec<{}>) -> Result<(), String> {{\n",
                id_field.field_type.to_rust_type()
            ));
            code.push_str("        for id in ids {\n");
            code.push_str("            self.delete(id)?;\n");
            code.push_str("        }\n");
            code.push_str("        Ok(())\n");
            code.push_str("    }\n\n");
        }
    }
}
