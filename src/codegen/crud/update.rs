use crate::ast::{FieldType, IndexType, Model};
use super::super::utils::{is_virtual_field, get_field_param_name, get_field_param_type};

pub struct UpdateGenerator;

impl UpdateGenerator {
    pub fn new() -> Self {
        UpdateGenerator
    }

    pub fn generate_update_method(&self, code: &mut String, model: &Model) {
        // Find the ID field
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!(
                "    pub fn update(&mut self, {}: {}",
                id_field.name,
                id_field.field_type.to_rust_type()
            ));

            // Parameters: all non-auto-generated, non-virtual fields
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }

                if !field.auto_generate {
                    let param_name = get_field_param_name(field);
                    let param_type = get_field_param_type(field);
                    code.push_str(&format!(", {}: {}", param_name, param_type));
                }
            }

            code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

            // Find the record
            code.push_str("        let idx = self.records.iter().enumerate()\n");
            code.push_str(&format!(
                "            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n",
                id_field.name, id_field.name
            ));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Remove old values from indexes
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

            // Remove old values from composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let old_tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let old_tuple = format!("({})", old_tuple_values.join(", "));
                code.push_str(&format!(
                    "        if let Some(indices) = self.{}_index.get_mut(&{}) {{\n",
                    index_name, old_tuple
                ));
                code.push_str("            indices.retain(|&i| i != idx);\n");
                code.push_str("        }\n");
            }

            // Check unique constraints for new values
            for field in &model.fields {
                if field.unique && !field.auto_generate {
                    let param_name = get_field_param_name(field);
                    code.push_str(&format!("        if self.{}_index.contains_key(&{}) && self.records[idx].{} != {} {{\n",
                        param_name, param_name, param_name, param_name));
                    code.push_str(&format!("            return Err(\"Unique constraint violation: {} already exists\".to_string());\n", param_name));
                    code.push_str("        }\n");
                }
            }

            // Update record
            code.push_str(&format!("        self.records[idx] = {} {{\n", model.name));
            code.push_str(&format!(
                "            {}: self.records[idx].{}.clone(),\n",
                id_field.name, id_field.name
            ));
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }

                if field.auto_generate && field.name != id_field.name {
                    // Preserve auto-generated fields (except ID which is already handled)
                    code.push_str(&format!(
                        "            {}: self.records[idx].{}.clone(),\n",
                        field.name, field.name
                    ));
                } else if !field.auto_generate {
                    // Use parameter values for non-auto-generated fields
                    let param_name = get_field_param_name(field);
                    code.push_str(&format!("            {},\n", param_name));
                }
            }
            code.push_str("        };\n\n");

            // Add new values to indexes
            for field in &model.fields {
                if is_virtual_field(field) {
                    continue;
                }

                let param_name = get_field_param_name(field);
                let is_fk =
                    matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!(
                        "        self.{}_index.insert(self.records[idx].{}.clone(), idx);\n",
                        param_name, param_name
                    ));
                } else if field.indexed || is_fk {
                    match field.index_type {
                        IndexType::Hash => {
                            code.push_str(&format!("        self.{}_index.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                                param_name, param_name));
                        }
                        IndexType::BTree => {
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        self.{}_btree.entry(ordered_float::OrderedFloat(self.records[idx].{})).or_insert_with(Vec::new).push(idx);\n",
                                    param_name, param_name));
                            } else {
                                code.push_str(&format!("        self.{}_btree.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                                    param_name, param_name));
                            }
                        }
                    }
                }
            }

            // Add new values to composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let new_tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let new_tuple = format!("({})", new_tuple_values.join(", "));
                code.push_str(&format!(
                    "        self.{}_index.entry({}).or_insert_with(Vec::new).push(idx);\n",
                    index_name, new_tuple
                ));
            }

            code.push_str("        Ok(self.records[idx].clone())\n");
            code.push_str("    }\n\n");
        }
    }
}
