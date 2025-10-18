use super::utils::is_virtual_field;
use crate::ast::{Field, FieldType, Model, RelationType, Struct};

pub struct ModelGenerator;

impl ModelGenerator {
    pub fn new() -> Self {
        ModelGenerator
    }

    /// Generate struct definition (Sprint 8)
    pub fn generate_struct_definition(&self, struct_def: &Struct) -> String {
        let mut code = String::new();

        code.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq)]\n"));
        code.push_str("#[repr(C)]\n"); // Ensure C layout for predictable memory layout
        code.push_str(&format!("pub struct {} {{\n", struct_def.name));

        for field in &struct_def.fields {
            let rust_type = field.field_type.to_rust_type();
            code.push_str(&format!("    pub {}: {},\n", field.name, rust_type));
        }

        code.push_str("}\n\n");

        // Generate helper methods for struct
        code.push_str(&format!("impl {} {{\n", struct_def.name));

        // Generate constructor
        code.push_str("    pub fn new(");
        for (i, field) in struct_def.fields.iter().enumerate() {
            if i > 0 {
                code.push_str(", ");
            }
            code.push_str(&format!(
                "{}: {}",
                field.name,
                field.field_type.to_rust_type()
            ));
        }
        code.push_str(") -> Self {\n");
        code.push_str(&format!("        {} {{\n", struct_def.name));
        for field in &struct_def.fields {
            code.push_str(&format!("            {},\n", field.name));
        }
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");

        code
    }

    pub fn generate_field_declaration(&self, field: &Field) -> String {
        // Skip virtual fields - they don't store data
        if is_virtual_field(field) {
            return String::new();
        }

        // For reference fields, generate the FK field name
        let (field_name, rust_type) = match &field.field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => {
                (format!("{}_id", field.name), "uuid::Uuid".to_string())
            }
            FieldType::Relation(RelationType::OptionalReference(_)) => (
                format!("{}_id", field.name),
                "Option<uuid::Uuid>".to_string(),
            ),
            _ => (field.name.clone(), field.field_type.to_rust_type()),
        };

        format!("    pub {}: {},", field_name, rust_type)
    }

    pub fn generate_struct(&self, model: &Model) -> String {
        let mut code = String::new();

        // Generate the main struct
        code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\n"));
        code.push_str(&format!("pub struct {} {{\n", model.name));

        for field in &model.fields {
            let field_decl = self.generate_field_declaration(field);
            if !field_decl.is_empty() {
                code.push_str(&field_decl);
                code.push('\n');
            }
        }

        code.push_str("}\n\n");

        code
    }
}
