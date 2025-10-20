use crate::ast::{
    ComponentProtocol, ComponentReference, Field, FieldType, RelationInclusion, Schema,
};
use crate::codegen::ir::IrSchema;

pub struct ComponentPropsGenerator;

impl ComponentPropsGenerator {
    pub fn new() -> Self {
        ComponentPropsGenerator
    }

    /// Generate component props types for all component fields in the schema using IR
    pub fn generate_props_types(&self, schema: &Schema) -> String {
        let ir_schema = IrSchema::from_ast(schema.clone());
        let mut content = String::new();

        content.push_str("// Auto-generated TypeScript component props types\n");
        content.push_str("// DO NOT EDIT - This file is generated from your schema\n\n");

        // Generate props for each model's component fields using IR
        for ir_model in &ir_schema.models {
            for ir_field in &ir_model.fields {
                if let FieldType::Component(component_ref) = &ir_field.field_type {
                    // Only generate props for TSX/JSX components (not API routes)
                    if matches!(
                        component_ref.protocol,
                        ComponentProtocol::Tsx | ComponentProtocol::Jsx
                    ) {
                        content.push_str(&self.generate_component_props(
                            &ir_model.name,
                            &ir_field.name,
                            component_ref,
                            &ir_model,
                            schema,
                        ));
                        content.push('\n');
                    }
                }
            }
        }

        content
    }

    /// Generate props type for a single component using IR
    fn generate_component_props(
        &self,
        model_name: &str,
        field_name: &str,
        component_ref: &ComponentReference,
        ir_model: &crate::codegen::ir::IrModel,
        schema: &Schema,
    ) -> String {
        let mut content = String::new();

        // Generate type name: UserCardProps
        let type_name = format!(
            "{}{}Props",
            model_name,
            Self::capitalize_first(field_name)
        );

        content.push_str(&format!("export type {} = {{\n", type_name));

        // Always include the data field with the model type
        content.push_str(&format!("  data: {};\n", model_name));

        // Check if model has computed fields using IR
        if !ir_model.computed_fields.is_empty() {
            content.push_str(&format!("  computed?: {}Computed;\n", model_name));
        }

        // Add relations based on the @relations directive using IR
        match &component_ref.relations {
            RelationInclusion::None => {
                // No relations included
            }
            RelationInclusion::All => {
                // Include all relation fields using IR's virtual_fields
                let relation_fields: Vec<_> = ir_model.virtual_fields
                    .iter()
                    .filter(|f| f.field_type.is_relation())
                    .collect();

                if !relation_fields.is_empty() {
                    content.push_str("  relations?: {\n");
                    for rel_field in relation_fields {
                        let rel_type = self.get_relation_type_name(&rel_field.field_type, schema);
                        content.push_str(&format!("    {}?: {};\n", rel_field.name, rel_type));
                    }
                    content.push_str("  };\n");
                }
            }
            RelationInclusion::Specific(fields) => {
                // Include specific relation fields using IR
                if !fields.is_empty() {
                    content.push_str("  relations?: {\n");
                    for field_name in fields {
                        // Find the field in IR's fields
                        if let Some(ir_field) = ir_model.fields.iter().find(|f| &f.name == field_name) {
                            if ir_field.field_type.is_relation() {
                                let rel_type =
                                    self.get_relation_type_name(&ir_field.field_type, schema);
                                content
                                    .push_str(&format!("    {}?: {};\n", ir_field.name, rel_type));
                            }
                        }
                    }
                    content.push_str("  };\n");
                }
            }
        }

        content.push_str("};\n");
        content
    }

    /// Get the TypeScript type name for a relation
    fn get_relation_type_name(&self, field_type: &FieldType, _schema: &Schema) -> String {
        use crate::ast::RelationType;

        match field_type {
            FieldType::Relation(rel_type) => match rel_type {
                RelationType::OneToMany(target) | RelationType::ManyToMany(target) => {
                    format!("{}[]", target)
                }
                RelationType::RequiredReference(target) => target.clone(),
                RelationType::OptionalReference(target) => format!("{} | null", target),
            },
            _ => "unknown".to_string(),
        }
    }

    /// Capitalize the first letter of a string
    fn capitalize_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().chain(chars).collect(),
        }
    }
}

