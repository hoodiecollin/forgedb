use crate::ast::{Field, FieldType, Model, RelationType, Schema};
use super::super::utils::{is_virtual_field, get_field_param_name, get_field_param_type};

pub struct ForeignKeyGenerator;

impl ForeignKeyGenerator {
    pub fn new() -> Self {
        ForeignKeyGenerator
    }

    pub fn generate_database_struct(&self, schema: &Schema) -> String {
        let mut code = String::new();

        code.push_str("pub struct Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "    pub {}: {}Storage,\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        code.push_str("}\n\n");

        code.push_str("impl Database {\n");
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str("        Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "            {}: {}Storage::new(),\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // Generate FK validation insert methods
        for model in &schema.models {
            self.generate_db_insert_with_fk_validation(&mut code, model, schema);
        }

        // Note: relation traversal methods will be added here by the traversal generator

        code.push_str("}\n\n");

        code
    }

    pub fn generate_db_insert_with_fk_validation(
        &self,
        code: &mut String,
        model: &Model,
        _schema: &Schema,
    ) {
        // Find FK fields in this model
        let fk_fields: Vec<&Field> = model
            .fields
            .iter()
            .filter(|f| matches!(&f.field_type, FieldType::Relation(rel) if rel.is_reference()))
            .collect();

        if fk_fields.is_empty() {
            return; // No FKs to validate
        }

        let method_name = format!("insert_{}", model.name.to_lowercase());
        code.push_str(&format!("    pub fn {}(&mut self", method_name));

        // Parameters: all fields except auto-generated and virtual
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

        // Validate each FK
        for field in &fk_fields {
            if let FieldType::Relation(rel) = &field.field_type {
                let target_model = rel.target_model();
                let fk_param = format!("{}_id", field.name);
                let storage_name = target_model.to_lowercase();

                match rel {
                    RelationType::RequiredReference(_) => {
                        code.push_str(&format!(
                            "        if self.{}.get({}).is_none() {{\n",
                            storage_name, fk_param
                        ));
                        code.push_str(&format!("            return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("        }\n");
                    }
                    RelationType::OptionalReference(_) => {
                        code.push_str(&format!("        if let Some(fk) = {} {{\n", fk_param));
                        code.push_str(&format!(
                            "            if self.{}.get(fk).is_none() {{\n",
                            storage_name
                        ));
                        code.push_str(&format!("                return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                    }
                    _ => {}
                }
            }
        }

        // Call the underlying storage insert
        code.push_str(&format!(
            "        self.{}.insert(",
            model.name.to_lowercase()
        ));
        let mut first = true;
        for field in &model.fields {
            if is_virtual_field(field) {
                continue;
            }
            if !field.auto_generate {
                if !first {
                    code.push_str(", ");
                }
                code.push_str(&get_field_param_name(field));
                first = false;
            }
        }
        code.push_str(")\n");
        code.push_str("    }\n\n");
    }
}
