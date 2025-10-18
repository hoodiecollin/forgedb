use crate::ast::{FieldType, Model};

pub struct ComputedAccessorGenerator;

impl ComputedAccessorGenerator {
    pub fn new() -> Self {
        ComputedAccessorGenerator
    }

    /// Generate helper methods for computed fields (Sprint 12)
    pub fn generate_computed_accessors(&self, code: &mut String, model: &Model) {
        let computed_fields: Vec<_> = model.fields.iter().filter(|f| f.is_computed).collect();
        let materialized_fields: Vec<_> =
            model.fields.iter().filter(|f| f.is_materialized).collect();

        // Generate materialized field accessors (Sprint 19)
        if !materialized_fields.is_empty() {
            let id_field = model.fields.iter().find(|f| f.auto_generate);
            if let Some(id_field) = id_field {
                if matches!(id_field.field_type, FieldType::Uuid) {
                    for field in &materialized_fields {
                        code.push_str(&format!(
                            "    /// Get the materialized value of '{}'\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn get_materialized_{}(&self, {}: {}) -> Option<{}> {{\n",
                            field.name,
                            id_field.name,
                            id_field.field_type.to_rust_type(),
                            field.field_type.to_rust_type()
                        ));
                        code.push_str(&format!(
                            "        self.materialized_{}.get(&{}).cloned()\n",
                            field.name, id_field.name
                        ));
                        code.push_str("    }\n\n");

                        // Add method to recompute materialized field
                        code.push_str(&format!(
                            "    /// Recompute and update the materialized value of '{}'\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn recompute_materialized_{}<F>(&mut self, {}: {}, compute_fn: F) -> Result<(), String>\n",
                            field.name,
                            id_field.name,
                            id_field.field_type.to_rust_type()
                        ));
                        code.push_str(&format!(
                            "    where F: FnOnce(&{}) -> {} {{\n",
                            model.name,
                            field.field_type.to_rust_type()
                        ));
                        code.push_str(&format!(
                            "        if let Some(record) = self.get({}) {{\n",
                            id_field.name
                        ));
                        code.push_str("            let value = compute_fn(&record);\n");
                        code.push_str(&format!(
                            "            self.materialized_{}.insert({}, value);\n",
                            field.name, id_field.name
                        ));
                        code.push_str("            Ok(())\n");
                        code.push_str("        } else {\n");
                        code.push_str("            Err(\"Record not found\".to_string())\n");
                        code.push_str("        }\n");
                        code.push_str("    }\n\n");
                    }
                }
            }
        }

        if computed_fields.is_empty() {
            return;
        }

        // Generate a method to get a record with computed fields
        // This method takes a trait implementation as a generic parameter
        let id_field = model.fields.iter().find(|f| f.auto_generate);
        if let Some(id_field) = id_field {
            code.push_str(&format!("    /// Get a record with its computed fields\n"));
            code.push_str(&format!(
                "    pub fn get_with_computed<C: {}Computed>(&self, {}: {}) -> Option<{}> {{\n",
                model.name,
                id_field.name,
                id_field.field_type.to_rust_type(),
                model.name
            ));

            code.push_str(&format!("        self.get({})\n", id_field.name));
            code.push_str("    }\n\n");

            // Generate a method to compute a specific field value
            for field in &computed_fields {
                code.push_str(&format!(
                    "    /// Compute the value of '{}' for a record\n",
                    field.name
                ));
                code.push_str(&format!(
                    "    pub fn compute_{}<C: {}Computed>(&self, {}: {}) -> Option<{}> {{\n",
                    field.name,
                    model.name,
                    id_field.name,
                    id_field.field_type.to_rust_type(),
                    field.field_type.to_rust_type()
                ));

                code.push_str(&format!(
                    "        self.get({}).map(|record| C::{}(&record))\n",
                    id_field.name, field.name
                ));
                code.push_str("    }\n\n");
            }
        }
    }
}
