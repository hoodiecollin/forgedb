use crate::ast::{FieldType, Model};

pub struct ListGenerator;

impl ListGenerator {
    pub fn new() -> Self {
        ListGenerator
    }

    pub fn generate_list_method(&self, code: &mut String, model: &Model) {
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        code.push_str(&format!(
            "    pub fn list(&self) -> Vec<{}> {{\n",
            model.name
        ));

        if model.soft_delete {
            // Filter out soft-deleted records by default
            if let Some(id_field) = id_field {
                if matches!(id_field.field_type, FieldType::Uuid) {
                    code.push_str("        self.records.iter().enumerate()\n");
                    code.push_str("            .filter(|(i, r)| !self.tombstones[*i] && !self.deleted_at.contains_key(&r.");
                    code.push_str(&format!("{}))\n", id_field.name));
                    code.push_str("            .map(|(_, r)| r.clone())\n");
                    code.push_str("            .collect()\n");
                    code.push_str("    }\n\n");

                    // Add list_with_deleted method
                    code.push_str(&format!(
                        "    pub fn list_with_deleted(&self, include_deleted: bool) -> Vec<{}> {{\n",
                        model.name
                    ));
                    code.push_str("        if include_deleted {\n");
                    code.push_str("            self.records.iter().enumerate()\n");
                    code.push_str("                .filter(|(i, _)| !self.tombstones[*i])\n");
                    code.push_str("                .map(|(_, r)| r.clone())\n");
                    code.push_str("                .collect()\n");
                    code.push_str("        } else {\n");
                    code.push_str("            self.list()\n");
                    code.push_str("        }\n");
                    code.push_str("    }\n\n");
                    return;
                }
            }
        }

        // Default list without soft delete
        code.push_str("        self.records.iter().enumerate()\n");
        code.push_str("            .filter(|(i, _)| !self.tombstones[*i])\n");
        code.push_str("            .map(|(_, r)| r.clone())\n");
        code.push_str("            .collect()\n");
        code.push_str("    }\n\n");
    }
}
