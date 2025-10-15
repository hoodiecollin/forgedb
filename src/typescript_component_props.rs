use crate::ast::{
    ComponentProtocol, ComponentReference, Field, FieldType, Model, RelationInclusion, Schema,
};

pub struct ComponentPropsGenerator;

impl ComponentPropsGenerator {
    pub fn new() -> Self {
        ComponentPropsGenerator
    }

    /// Generate component props types for all component fields in the schema
    pub fn generate_props_types(&self, schema: &Schema) -> String {
        let mut content = String::new();

        content.push_str("// Auto-generated TypeScript component props types\n");
        content.push_str("// DO NOT EDIT - This file is generated from your schema\n\n");

        // Generate props for each model's component fields
        for model in &schema.models {
            for field in &model.fields {
                if let FieldType::Component(component_ref) = &field.field_type {
                    // Only generate props for TSX/JSX components (not API routes)
                    if matches!(
                        component_ref.protocol,
                        ComponentProtocol::Tsx | ComponentProtocol::Jsx
                    ) {
                        content.push_str(&self.generate_component_props(
                            &model.name,
                            &field.name,
                            component_ref,
                            &model.fields,
                            schema,
                        ));
                        content.push('\n');
                    }
                }
            }
        }

        content
    }

    /// Generate props type for a single component
    fn generate_component_props(
        &self,
        model_name: &str,
        field_name: &str,
        component_ref: &ComponentReference,
        model_fields: &[Field],
        schema: &Schema,
    ) -> String {
        let mut content = String::new();

        // Generate type name: UserCardProps
        let type_name = format!(
            "{}{}Props",
            model_name,
            Self::capitalize_first(&field_name)
        );

        content.push_str(&format!("export type {} = {{\n", type_name));

        // Always include the data field with the model type
        content.push_str(&format!("  data: {};\n", model_name));

        // Check if model has computed fields
        if model_fields.iter().any(|f| f.is_computed) {
            content.push_str(&format!("  computed?: {}Computed;\n", model_name));
        }

        // Add relations based on the @relations directive
        match &component_ref.relations {
            RelationInclusion::None => {
                // No relations included
            }
            RelationInclusion::All => {
                // Include all relation fields
                let relation_fields: Vec<_> = model_fields
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
                // Include specific relation fields
                if !fields.is_empty() {
                    content.push_str("  relations?: {\n");
                    for field_name in fields {
                        // Find the field in the model
                        if let Some(field) = model_fields.iter().find(|f| &f.name == field_name) {
                            if field.field_type.is_relation() {
                                let rel_type =
                                    self.get_relation_type_name(&field.field_type, schema);
                                content
                                    .push_str(&format!("    {}?: {};\n", field.name, rel_type));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{IndexType, RelationType};

    #[test]
    fn test_generate_basic_props() {
        let schema = Schema {
            structs: vec![],
            models: vec![Model {
                name: "User".to_string(),
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "email".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "card".to_string(),
                        field_type: FieldType::Component(ComponentReference {
                            protocol: ComponentProtocol::Tsx,
                            path: "components/user/card".to_string(),
                            relations: RelationInclusion::None,
                        }),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                soft_delete: false,
            }],
        };

        let generator = ComponentPropsGenerator::new();
        let output = generator.generate_props_types(&schema);

        assert!(output.contains("export type UserCardProps"));
        assert!(output.contains("data: User;"));
    }

    #[test]
    fn test_generate_props_with_relations() {
        let schema = Schema {
            structs: vec![],
            models: vec![Model {
                name: "User".to_string(),
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "posts".to_string(),
                        field_type: FieldType::Relation(RelationType::OneToMany("Post".to_string())),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "card".to_string(),
                        field_type: FieldType::Component(ComponentReference {
                            protocol: ComponentProtocol::Tsx,
                            path: "components/user/card".to_string(),
                            relations: RelationInclusion::Specific(vec!["posts".to_string()]),
                        }),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                soft_delete: false,
            }],
        };

        let generator = ComponentPropsGenerator::new();
        let output = generator.generate_props_types(&schema);

        assert!(output.contains("export type UserCardProps"));
        assert!(output.contains("data: User;"));
        assert!(output.contains("relations?"));
        assert!(output.contains("posts?: Post[]"));
    }
}
