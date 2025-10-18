use crate::ast::{Field, FieldType, IndexType, Model};
use super::super::utils::{is_virtual_field, get_field_param_name, get_field_param_type};
use super::super::validation_gen::ValidationGenerator;

pub struct InsertGenerator {
    validation_gen: ValidationGenerator,
}

impl InsertGenerator {
    pub fn new() -> Self {
        InsertGenerator {
            validation_gen: ValidationGenerator::new(),
        }
    }

    pub fn generate_insert_method(&self, code: &mut String, model: &Model) {
        // Find unique fields
        let unique_fields: Vec<&Field> = model.fields.iter().filter(|f| f.unique).collect();

        code.push_str("    pub fn insert(&mut self");

        // Parameters: all fields except auto-generated and virtual ones
        for field in &model.fields {
            if is_virtual_field(field) {
                continue; // Skip virtual fields
            }

            if !field.auto_generate {
                let param_name = get_field_param_name(field);
                let param_type = get_field_param_type(field);
                code.push_str(&format!(", {}: {}", param_name, param_type));
            }
        }

        code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

        // Validate field constraints
        for field in &model.fields {
            if !field.auto_generate && !field.constraints.is_empty() {
                let validation = self.validation_gen.generate_field_validation(field);
                if !validation.is_empty() {
                    code.push_str(&validation);
                }
            }
        }

        // Check unique constraints
        for field in &unique_fields {
            code.push_str(&format!(
                "        if self.{}_index.contains_key(&{}) {{\n",
                field.name, field.name
            ));
            code.push_str(&format!("            return Err(\"Unique constraint violation: {} already exists\".to_string());\n", field.name));
            code.push_str("        }\n");
        }

        // Generate auto-generated fields
        for field in &model.fields {
            if field.auto_generate {
                match field.field_type {
                    FieldType::U32 | FieldType::U64 => {
                        code.push_str(&format!("        let {} = self.next_id;\n", field.name));
                        code.push_str("        self.next_id += 1;\n");
                    }
                    FieldType::Uuid => {
                        code.push_str(&format!("        let {} = Uuid::new_v4();\n", field.name));
                    }
                    FieldType::Timestamp => {
                        code.push_str(&format!("        let {} = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;\n", field.name));
                    }
                    _ => {}
                }
            }
        }

        // Create record
        code.push_str(&format!("        let record = {} {{\n", model.name));
        for field in &model.fields {
            if is_virtual_field(field) {
                continue; // Skip virtual fields
            }

            let field_name = get_field_param_name(field);
            code.push_str(&format!("            {},\n", field_name));
        }
        code.push_str("        };\n\n");

        // Add to indexes
        code.push_str("        let row_index = self.records.len();\n");
        for field in &model.fields {
            if is_virtual_field(field) {
                continue;
            }

            let field_name = get_field_param_name(field);
            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                // Unique index: map value to single row index
                code.push_str(&format!(
                    "        self.{}_index.insert(record.{}.clone(), row_index);\n",
                    field_name, field_name
                ));
            } else if field.indexed || is_fk {
                match field.index_type {
                    IndexType::Hash => {
                        // Hash index: append row index to list of indices
                        code.push_str(&format!("        self.{}_index.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                            field_name, field_name));
                    }
                    IndexType::BTree => {
                        // B-tree index: append row index to list of indices
                        if matches!(field.field_type, FieldType::F64) {
                            code.push_str(&format!("        self.{}_btree.entry(ordered_float::OrderedFloat(record.{})).or_insert_with(Vec::new).push(row_index);\n",
                                field_name, field_name));
                        } else {
                            code.push_str(&format!("        self.{}_btree.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                                field_name, field_name));
                        }
                    }
                }
            }
        }

        // Add to composite indexes
        for comp_idx in &model.composite_indexes {
            let index_name = comp_idx.fields.join("_");
            let field_values: Vec<String> = comp_idx
                .fields
                .iter()
                .map(|fname| {
                    let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                    let field_name = get_field_param_name(field);
                    format!("record.{}.clone()", field_name)
                })
                .collect();
            let tuple_value = format!("({})", field_values.join(", "));
            code.push_str(&format!(
                "        self.{}_index.entry({}).or_insert_with(Vec::new).push(row_index);\n",
                index_name, tuple_value
            ));
        }

        // Add to full-text indexes (Sprint 18)
        // Get the ID field to use as document ID
        let id_field = model.fields.iter().find(|f| f.auto_generate);
        for field in &model.fields {
            if field.fulltext_indexed {
                if let Some(id_field) = id_field {
                    if matches!(id_field.field_type, FieldType::Uuid) {
                        code.push_str(&format!("        self.{}_fulltext.write().unwrap().add_document(record.{}, &record.{});\n",
                            field.name, id_field.name, field.name));
                    }
                }
            }
        }

        // Add record
        code.push_str("        self.records.push(record.clone());\n");
        code.push_str("        self.tombstones.push(false);\n");

        // Compute and store materialized fields (Sprint 19)
        let materialized_fields: Vec<&Field> =
            model.fields.iter().filter(|f| f.is_materialized).collect();
        if !materialized_fields.is_empty() {
            if let Some(id_field) = id_field {
                if matches!(id_field.field_type, FieldType::Uuid) {
                    code.push_str("\n        // Compute materialized fields\n");
                    for field in &materialized_fields {
                        code.push_str(&format!(
                            "        // TODO: Implement computation for materialized field '{}'\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "        // Example: let {} = compute_{}(&record);\n",
                            field.name, field.name
                        ));
                        code.push_str(&format!(
                            "        // self.materialized_{}.insert(record.{}, {});\n",
                            field.name, id_field.name, field.name
                        ));
                    }
                }
            }
        }

        code.push_str("        Ok(record)\n");
        code.push_str("    }\n\n");
    }
}
