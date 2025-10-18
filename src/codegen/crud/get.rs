use crate::ast::{FieldType, Model};

pub struct GetGenerator;

impl GetGenerator {
    pub fn new() -> Self {
        GetGenerator
    }

    pub fn generate_get_method(&self, code: &mut String, model: &Model) {
        // Find the ID field (first field with auto_generate)
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!(
                "    pub fn get(&self, {}: {}) -> Option<{}> {{\n",
                id_field.name,
                id_field.field_type.to_rust_type(),
                model.name
            ));

            if model.soft_delete && matches!(id_field.field_type, FieldType::Uuid) {
                // Filter out soft-deleted records
                code.push_str("        self.records.iter().enumerate()\n");
                code.push_str(&format!("            .find(|(i, r)| !self.tombstones[*i] && !self.deleted_at.contains_key(&r.{}) && r.", id_field.name));
                code.push_str(&format!("{} == {})\n", id_field.name, id_field.name));
                code.push_str("            .map(|(_, r)| r.clone())\n");
                code.push_str("    }\n\n");
            } else {
                code.push_str("        self.records.iter().enumerate()\n");
                code.push_str("            .find(|(i, r)| !self.tombstones[*i] && r.");
                code.push_str(&format!("{} == {})\n", id_field.name, id_field.name));
                code.push_str("            .map(|(_, r)| r.clone())\n");
                code.push_str("    }\n\n");
            }
        }
    }
}
