//! Intermediate Representation (IR) for schema models
//!
//! Provides a processed view of schema models with pre-computed flags
//! and classifications to avoid duplicating filters across generators.

use crate::ast::{Field, FieldType, Model, Schema};
use crate::codegen::semantics;

/// Intermediate representation of a field with pre-computed flags
#[derive(Debug, Clone)]
pub struct IrField {
    /// Original field name
    pub name: String,
    /// Name used for storage (e.g., "author" -> "author_id" for relations)
    pub name_for_storage: String,
    /// Original field type from AST
    pub field_type: FieldType,
    /// Whether this field is virtual (doesn't store data)
    pub is_virtual: bool,
    /// Whether this field is computed
    pub is_computed: bool,
    /// Whether this field is optional
    pub is_optional: bool,
    /// Whether this field is auto-generated
    pub is_auto_generate: bool,
    /// Whether this field is unique
    pub is_unique: bool,
    /// Whether this field is indexed
    pub is_indexed: bool,
    /// Original field reference (for constraints, etc.)
    pub original: Field,
}

impl IrField {
    /// Create an IrField from an AST Field
    pub fn from_ast(field: Field) -> Self {
        let is_virtual = semantics::is_virtual_field(&field);
        let is_computed = field.is_computed;
        let is_optional = semantics::is_optional_field(&field);
        let name_for_storage = semantics::relation_field_name(&field);
        
        IrField {
            name: field.name.clone(),
            name_for_storage,
            field_type: field.field_type.clone(),
            is_virtual,
            is_computed,
            is_optional,
            is_auto_generate: field.auto_generate,
            is_unique: field.unique,
            is_indexed: field.indexed,
            original: field,
        }
    }
}

/// Intermediate representation of a model with categorized fields
#[derive(Debug, Clone)]
pub struct IrModel {
    /// Model name
    pub name: String,
    /// All fields
    pub fields: Vec<IrField>,
    /// Fields that are stored in the database
    pub stored_fields: Vec<IrField>,
    /// Computed fields
    pub computed_fields: Vec<IrField>,
    /// Virtual fields (relations, components)
    pub virtual_fields: Vec<IrField>,
    /// ID field (if any)
    pub id_field: Option<IrField>,
    /// Original model reference
    pub original: Model,
}

impl IrModel {
    /// Create an IrModel from an AST Model
    pub fn from_ast(model: Model) -> Self {
        let fields: Vec<IrField> = model.fields.iter().cloned().map(IrField::from_ast).collect();
        
        let stored_fields: Vec<IrField> = fields
            .iter()
            .filter(|f| !f.is_virtual && !f.is_computed)
            .cloned()
            .collect();
        
        let computed_fields: Vec<IrField> = fields
            .iter()
            .filter(|f| f.is_computed)
            .cloned()
            .collect();
        
        let virtual_fields: Vec<IrField> = fields
            .iter()
            .filter(|f| f.is_virtual)
            .cloned()
            .collect();
        
        let id_field = fields
            .iter()
            .find(|f| f.is_auto_generate && matches!(f.field_type, FieldType::Uuid))
            .cloned();
        
        IrModel {
            name: model.name.clone(),
            fields,
            stored_fields,
            computed_fields,
            virtual_fields,
            id_field,
            original: model,
        }
    }
    
    /// Get the API resource name (pluralized, lowercase)
    pub fn relation_name_for_api(&self) -> String {
        use crate::codegen::naming;
        let lower = self.name.to_lowercase();
        naming::pluralize(&lower)
    }
    
    /// Get fields that should appear in Create requests
    pub fn create_request_fields(&self) -> Vec<&IrField> {
        self.fields
            .iter()
            .filter(|f| !f.is_auto_generate && !f.is_virtual && !f.is_computed)
            .collect()
    }
    
    /// Get fields that should appear in Update requests
    pub fn update_request_fields(&self) -> Vec<&IrField> {
        self.fields
            .iter()
            .filter(|f| !f.is_auto_generate && !f.is_virtual && !f.is_computed)
            .collect()
    }
    
    /// Get fields that should appear in Response types
    pub fn response_fields(&self) -> Vec<&IrField> {
        self.fields
            .iter()
            .filter(|f| !f.is_virtual)
            .collect()
    }
}

/// Intermediate representation of the entire schema
#[derive(Debug, Clone)]
pub struct IrSchema {
    /// All models
    pub models: Vec<IrModel>,
    /// Original schema reference
    pub original: Schema,
}

impl IrSchema {
    /// Create an IrSchema from an AST Schema
    pub fn from_ast(schema: Schema) -> Self {
        let models = schema.models.iter().cloned().map(IrModel::from_ast).collect();
        
        IrSchema {
            models,
            original: schema,
        }
    }
    
    /// Find a model by name
    pub fn find_model(&self, name: &str) -> Option<&IrModel> {
        self.models.iter().find(|m| m.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{IndexType, RelationType};

    fn create_test_field(name: &str, field_type: FieldType, auto_generate: bool, is_computed: bool) -> Field {
        Field {
            name: name.to_string(),
            field_type,
            auto_generate,
            unique: false,
            indexed: false,
            constraints: vec![],
            index_type: IndexType::Hash,
            is_computed,
            fulltext_indexed: false,
            is_materialized: false,
        }
    }

    #[test]
    fn test_ir_field_from_ast() {
        let field = create_test_field("name", FieldType::String, false, false);
        let ir_field = IrField::from_ast(field);
        
        assert_eq!(ir_field.name, "name");
        assert_eq!(ir_field.name_for_storage, "name");
        assert!(!ir_field.is_virtual);
        assert!(!ir_field.is_computed);
    }

    #[test]
    fn test_ir_field_relation_storage_name() {
        let field = create_test_field(
            "author",
            FieldType::Relation(RelationType::RequiredReference("User".to_string())),
            false,
            false,
        );
        let ir_field = IrField::from_ast(field);
        
        assert_eq!(ir_field.name, "author");
        assert_eq!(ir_field.name_for_storage, "author_id");
        assert!(!ir_field.is_virtual);
    }

    #[test]
    fn test_ir_model_from_ast() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![
                create_test_field("id", FieldType::Uuid, true, false),
                create_test_field("name", FieldType::String, false, false),
                create_test_field("email", FieldType::String, false, false),
                create_test_field("full_name", FieldType::String, false, true),
                create_test_field(
                    "posts",
                    FieldType::Relation(RelationType::OneToMany("Post".to_string())),
                    false,
                    false,
                ),
            ],
            composite_indexes: vec![],
            soft_delete: false,
        };
        
        let ir_model = IrModel::from_ast(model);
        
        assert_eq!(ir_model.name, "User");
        assert_eq!(ir_model.fields.len(), 5);
        assert_eq!(ir_model.stored_fields.len(), 3); // id, name, email
        assert_eq!(ir_model.computed_fields.len(), 1); // full_name
        assert_eq!(ir_model.virtual_fields.len(), 1); // posts
        assert!(ir_model.id_field.is_some());
    }

    #[test]
    fn test_ir_model_request_fields() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![
                create_test_field("id", FieldType::Uuid, true, false),
                create_test_field("name", FieldType::String, false, false),
                create_test_field("email", FieldType::String, false, false),
                create_test_field("full_name", FieldType::String, false, true),
            ],
            composite_indexes: vec![],
            soft_delete: false,
        };
        
        let ir_model = IrModel::from_ast(model);
        
        let create_fields = ir_model.create_request_fields();
        assert_eq!(create_fields.len(), 2); // name, email (no id, no computed)
        
        let update_fields = ir_model.update_request_fields();
        assert_eq!(update_fields.len(), 2); // name, email (no id, no computed)
    }

    #[test]
    fn test_ir_schema_from_ast() {
        let schema = Schema {
            structs: vec![],
            models: vec![
                Model {
                    name: "User".to_string(),
                    fields: vec![create_test_field("name", FieldType::String, false, false)],
                    composite_indexes: vec![],
                    soft_delete: false,
                },
                Model {
                    name: "Post".to_string(),
                    fields: vec![create_test_field("title", FieldType::String, false, false)],
                    composite_indexes: vec![],
                    soft_delete: false,
                },
            ],
        };
        
        let ir_schema = IrSchema::from_ast(schema);
        
        assert_eq!(ir_schema.models.len(), 2);
        assert!(ir_schema.find_model("User").is_some());
        assert!(ir_schema.find_model("Post").is_some());
        assert!(ir_schema.find_model("Comment").is_none());
    }
}
