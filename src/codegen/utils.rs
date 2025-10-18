use crate::ast::{Field, FieldType, RelationType};

/// Check if a field is virtual (OneToMany, ManyToMany, or Component) and doesn't need storage
pub fn is_virtual_field(field: &Field) -> bool {
    matches!(
        &field.field_type,
        FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_))
            | FieldType::Component(_)
    )
}

/// Get the parameter name for a field (handles FK fields)
pub fn get_field_param_name(field: &Field) -> String {
    match &field.field_type {
        FieldType::Relation(rel) if rel.is_reference() => format!("{}_id", field.name),
        _ => field.name.clone(),
    }
}

/// Get the parameter type for a field (handles FK types)
pub fn get_field_param_type(field: &Field) -> String {
    match &field.field_type {
        FieldType::Relation(RelationType::RequiredReference(_)) => "uuid::Uuid".to_string(),
        FieldType::Relation(RelationType::OptionalReference(_)) => "Option<uuid::Uuid>".to_string(),
        _ => field.field_type.to_rust_type(),
    }
}
