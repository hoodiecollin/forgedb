use crate::ast::{FieldType, IndexType, Model};
use super::super::utils::{is_virtual_field, get_field_param_name};
use super::batch::BatchGenerator;

pub struct DeleteGenerator {
    batch_gen: BatchGenerator,
}

impl DeleteGenerator {
    pub fn new() -> Self {
        DeleteGenerator {
            batch_gen: BatchGenerator::new(),
        }
    }

    pub fn generate_delete_method(&self, code: &mut String, model: &Model) {
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!(
                "    pub fn delete(&mut self, {}: {}) -> Result<(), String> {{\n",
                id_field.name,
                id_field.field_type.to_rust_type()
            ));

            if model.soft_delete {
                // Soft delete: set deleted_at timestamp
                if matches!(id_field.field_type, FieldType::Uuid) {
                    code.push_str(
                        "        // Soft delete: mark record as deleted with timestamp\n",
                    );
                    code.push_str("        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;\n");
                    code.push_str(&format!(
                        "        self.deleted_at.insert({}, timestamp);\n",
                        id_field.name
                    ));
                    code.push_str("        Ok(())\n");
                    code.push_str("    }\n\n");

                    // Generate restore and batch methods even for soft delete
                    // Add restore method for soft delete (Sprint 19)
                    code.push_str(&format!("    /// Restore a soft-deleted record\n"));
                    code.push_str(&format!(
                        "    pub fn restore(&mut self, {}: {}) -> Result<(), String> {{\n",
                        id_field.name,
                        id_field.field_type.to_rust_type()
                    ));
                    code.push_str(&format!(
                        "        if self.deleted_at.remove(&{}).is_some() {{\n",
                        id_field.name
                    ));
                    code.push_str("            Ok(())\n");
                    code.push_str("        } else {\n");
                    code.push_str(
                        "            Err(\"Record not found or not deleted\".to_string())\n",
                    );
                    code.push_str("        }\n");
                    code.push_str("    }\n\n");

                    // Add batch operations (Sprint 19)
                    self.batch_gen.generate_batch_methods(code, model);
                    return;
                }
            }

            // Hard delete: Find the record
            code.push_str("        let idx = self.records.iter().enumerate()\n");
            code.push_str(&format!(
                "            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n",
                id_field.name, id_field.name
            ));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Mark as deleted (tombstone)
            code.push_str("        self.tombstones[idx] = true;\n\n");

            // Remove from indexes (optional optimization to free memory)
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }

                let param_name = get_field_param_name(field);
                let is_fk =
                    matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!(
                        "        self.{}_index.remove(&self.records[idx].{});\n",
                        param_name, param_name
                    ));
                } else if field.indexed || is_fk {
                    match field.index_type {
                        IndexType::Hash => {
                            code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                        IndexType::BTree => {
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&ordered_float::OrderedFloat(self.records[idx].{})) {{\n", param_name, param_name));
                            } else {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            }
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                    }
                }
            }

            // Remove from composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let tuple = format!("({})", tuple_values.join(", "));
                code.push_str(&format!(
                    "        if let Some(indices) = self.{}_index.get_mut(&{}) {{\n",
                    index_name, tuple
                ));
                code.push_str("            indices.retain(|&i| i != idx);\n");
                code.push_str("        }\n");
            }

            code.push_str("        Ok(())\n");
            code.push_str("    }\n\n");

            // Add restore method for soft delete (Sprint 19)
            if model.soft_delete && matches!(id_field.field_type, FieldType::Uuid) {
                code.push_str(&format!("    /// Restore a soft-deleted record\n"));
                code.push_str(&format!(
                    "    pub fn restore(&mut self, {}: {}) -> Result<(), String> {{\n",
                    id_field.name,
                    id_field.field_type.to_rust_type()
                ));
                code.push_str(&format!(
                    "        if self.deleted_at.remove(&{}).is_some() {{\n",
                    id_field.name
                ));
                code.push_str("            Ok(())\n");
                code.push_str("        } else {\n");
                code.push_str("            Err(\"Record not found or not deleted\".to_string())\n");
                code.push_str("        }\n");
                code.push_str("    }\n\n");
            }

            // Add batch operations (Sprint 19)
            self.batch_gen.generate_batch_methods(code, model);
        }
    }
}
