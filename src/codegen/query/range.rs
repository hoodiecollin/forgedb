use crate::ast::{CompositeIndex, Field, FieldType, Model};
use super::super::utils::{get_field_param_name, get_field_param_type};

pub struct RangeQueryGenerator;

impl RangeQueryGenerator {
    pub fn new() -> Self {
        RangeQueryGenerator
    }

    pub fn generate_range_query_methods(&self, code: &mut String, field: &Field, model: &Model) {
        let param_name = get_field_param_name(field);
        let param_type = get_field_param_type(field);

        // find_by_X_range(min, max)
        code.push_str(&format!(
            "    pub fn find_by_{}_range(&self, min: {}, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(ordered_float::OrderedFloat(min)..=ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(min..=max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_gt(min) - greater than
        code.push_str(&format!(
            "    pub fn find_by_{}_gt(&self, min: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range((std::ops::Bound::Excluded(ordered_float::OrderedFloat(min)), std::ops::Bound::Unbounded)) {{\n", param_name));
        } else {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range((std::ops::Bound::Excluded(min), std::ops::Bound::Unbounded)) {{\n", param_name));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_gte(min) - greater than or equal
        code.push_str(&format!(
            "    pub fn find_by_{}_gte(&self, min: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(ordered_float::OrderedFloat(min)..) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(min..) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_lt(max) - less than
        code.push_str(&format!(
            "    pub fn find_by_{}_lt(&self, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(..ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(..max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_lte(max) - less than or equal
        code.push_str(&format!(
            "    pub fn find_by_{}_lte(&self, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(..=ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(..=max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");
    }

    pub fn generate_composite_find_by_method(
        &self,
        code: &mut String,
        comp_idx: &CompositeIndex,
        model: &Model,
    ) {
        let method_name = format!(
            "find_by_{}",
            comp_idx
                .fields
                .iter()
                .map(|f| format!("{}", f))
                .collect::<Vec<_>>()
                .join("_and_")
        );
        let index_name = comp_idx.fields.join("_");

        // Generate parameters
        let params: Vec<String> = comp_idx
            .fields
            .iter()
            .map(|fname| {
                let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                let param_name = get_field_param_name(field);
                let param_type = get_field_param_type(field);
                format!("{}: {}", param_name, param_type)
            })
            .collect();

        code.push_str(&format!(
            "    pub fn {}(&self, {}) -> Vec<{}> {{\n",
            method_name,
            params.join(", "),
            model.name
        ));

        // Generate tuple key
        let tuple_values: Vec<String> = comp_idx
            .fields
            .iter()
            .map(|fname| {
                let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                get_field_param_name(field)
            })
            .collect();
        let tuple_key = format!("({})", tuple_values.join(", "));

        code.push_str(&format!(
            "        if let Some(indices) = self.{}_index.get(&{}) {{\n",
            index_name, tuple_key
        ));
        code.push_str("            return indices.iter()\n");
        code.push_str("                .filter(|&&idx| !self.tombstones[idx])\n");
        code.push_str("                .map(|&idx| self.records[idx].clone())\n");
        code.push_str("                .collect();\n");
        code.push_str("        }\n");
        code.push_str("        Vec::new()\n");
        code.push_str("    }\n\n");
    }
}
