use crate::ast::{Field, FieldType, Model};

pub struct ComputedTraitGenerator;

impl ComputedTraitGenerator {
    pub fn new() -> Self {
        ComputedTraitGenerator
    }

    /// Generate computed field trait for a model (Sprint 12)
    pub fn generate_computed_trait(&self, model: &Model) -> String {
        let computed_fields: Vec<&Field> = model.fields.iter().filter(|f| f.is_computed).collect();

        if computed_fields.is_empty() {
            return String::new();
        }

        let mut code = String::new();

        code.push_str(&format!("/// Computed fields trait for {}\n", model.name));
        code.push_str(&format!("pub trait {}Computed {{\n", model.name));

        for field in &computed_fields {
            // Determine parameters based on field dependencies
            // For now, we pass a reference to the entire model instance
            code.push_str(&format!("    /// Compute the value of '{}'\n", field.name));
            code.push_str(&format!(
                "    fn {}(instance: &{}) -> {};\n",
                field.name,
                model.name,
                field.field_type.to_rust_type()
            ));
        }

        code.push_str("}\n\n");

        // Generate a default stub implementation
        code.push_str(&format!(
            "/// Default stub implementation for {}Computed\n",
            model.name
        ));
        code.push_str(&format!("pub struct Default{}Computed;\n\n", model.name));
        code.push_str(&format!(
            "impl {}Computed for Default{}Computed {{\n",
            model.name, model.name
        ));

        for field in &computed_fields {
            code.push_str(&format!(
                "    fn {}(instance: &{}) -> {} {{\n",
                field.name,
                model.name,
                field.field_type.to_rust_type()
            ));
            code.push_str("        // TODO: Implement computation logic\n");

            // Generate a placeholder return value based on type
            let default_value = match &field.field_type {
                FieldType::String => "String::new()".to_string(),
                FieldType::U32 => "0u32".to_string(),
                FieldType::U64 => "0u64".to_string(),
                FieldType::I32 => "0i32".to_string(),
                FieldType::I64 => "0i64".to_string(),
                FieldType::F64 => "0.0f64".to_string(),
                FieldType::Bool => "false".to_string(),
                FieldType::Uuid => "Uuid::nil()".to_string(),
                _ => "unimplemented!()".to_string(),
            };
            code.push_str(&format!("        {}\n", default_value));
            code.push_str("    }\n");
        }

        code.push_str("}\n\n");

        code
    }
}
