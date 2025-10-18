use crate::ast::{FieldType, IndexType, Model};
use super::super::utils::{is_virtual_field, get_field_param_name, get_field_param_type};
use super::range::RangeQueryGenerator;

pub struct FindByGenerator {
    range_gen: RangeQueryGenerator,
}

impl FindByGenerator {
    pub fn new() -> Self {
        FindByGenerator {
            range_gen: RangeQueryGenerator::new(),
        }
    }

    pub fn generate_find_by_methods(&self, code: &mut String, model: &Model) {
        // Generate find_by_X for indexed or unique fields, and FK fields
        for field in &model.fields {
            if is_virtual_field(field) {
                continue;
            }

            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.indexed || field.unique || is_fk {
                let param_name = get_field_param_name(field);
                let param_type = get_field_param_type(field);
                let method_name = format!("find_by_{}", param_name);

                code.push_str(&format!(
                    "    pub fn {}(&self, {}: {}) -> Vec<{}> {{\n",
                    method_name, param_name, param_type, model.name
                ));

                if field.unique {
                    // Unique index: O(1) lookup, returns 0 or 1 results
                    code.push_str(&format!(
                        "        if let Some(&idx) = self.{}_index.get(&{}) {{\n",
                        param_name, param_name
                    ));
                    code.push_str("            if !self.tombstones[idx] {\n");
                    code.push_str("                return vec![self.records[idx].clone()];\n");
                    code.push_str("            }\n");
                    code.push_str("        }\n");
                    code.push_str("        Vec::new()\n");
                } else {
                    match field.index_type {
                        IndexType::Hash => {
                            // Hash index: O(1) lookup, may return multiple results
                            code.push_str(&format!(
                                "        if let Some(indices) = self.{}_index.get(&{}) {{\n",
                                param_name, param_name
                            ));
                            code.push_str("            return indices.iter()\n");
                            code.push_str(
                                "                .filter(|&&idx| !self.tombstones[idx])\n",
                            );
                            code.push_str(
                                "                .map(|&idx| self.records[idx].clone())\n",
                            );
                            code.push_str("                .collect();\n");
                            code.push_str("        }\n");
                            code.push_str("        Vec::new()\n");
                        }
                        IndexType::BTree => {
                            // B-tree index: O(log n) lookup
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get(&ordered_float::OrderedFloat({})) {{\n", param_name, param_name));
                            } else {
                                code.push_str(&format!(
                                    "        if let Some(indices) = self.{}_btree.get(&{}) {{\n",
                                    param_name, param_name
                                ));
                            }
                            code.push_str("            return indices.iter()\n");
                            code.push_str(
                                "                .filter(|&&idx| !self.tombstones[idx])\n",
                            );
                            code.push_str(
                                "                .map(|&idx| self.records[idx].clone())\n",
                            );
                            code.push_str("                .collect();\n");
                            code.push_str("        }\n");
                            code.push_str("        Vec::new()\n");
                        }
                    }
                }

                code.push_str("    }\n\n");

                // Generate range query methods for B-tree indexed fields
                if !field.unique && matches!(field.index_type, IndexType::BTree) {
                    self.range_gen.generate_range_query_methods(code, field, model);
                }
            }
        }

        // Generate find_by methods for composite indexes
        for comp_idx in &model.composite_indexes {
            self.range_gen.generate_composite_find_by_method(code, comp_idx, model);
        }
    }
}
