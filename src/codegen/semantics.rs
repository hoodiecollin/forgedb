//! Semantic analysis utilities for schema fields and models
//!
//! Provides shared helpers to extract information from the AST and convert
//! field types to various target languages.

use crate::ast::{Field, FieldType, Model, RelationType};
use proc_macro2::TokenStream;
use quote::quote;

/// Check if a field is virtual (doesn't store data in the database)
pub fn is_virtual_field(field: &Field) -> bool {
    matches!(
        &field.field_type,
        FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_))
            | FieldType::Component(_)
    )
}

/// Check if a field is optional
pub fn is_optional_field(field: &Field) -> bool {
    matches!(
        &field.field_type,
        FieldType::OptionalStructType(_) 
            | FieldType::Relation(RelationType::OptionalReference(_))
    )
}

/// Get the ID field from a model (typically the auto-generated UUID field)
pub fn id_field(model: &Model) -> Option<&Field> {
    model.fields.iter().find(|f| f.auto_generate && matches!(f.field_type, FieldType::Uuid))
}

/// Get all computed fields from a model
pub fn computed_fields(model: &Model) -> Vec<&Field> {
    model.fields.iter().filter(|f| f.is_computed).collect()
}

/// Generate the relation field name for storage (e.g., "author" -> "author_id")
pub fn relation_field_name(field: &Field) -> String {
    if let FieldType::Relation(rel_type) = &field.field_type {
        match rel_type {
            RelationType::RequiredReference(_) | RelationType::OptionalReference(_) => {
                format!("{}_id", field.name)
            }
            _ => field.name.clone(),
        }
    } else {
        field.name.clone()
    }
}

/// Map a FieldType to Rust tokens (for use with quote!)
pub fn map_field_type_to_rust_tokens(field_type: &FieldType, for_response: bool) -> TokenStream {
    match field_type {
        FieldType::U32 => quote! { u32 },
        FieldType::U64 => quote! { u64 },
        FieldType::I32 => quote! { i32 },
        FieldType::I64 => quote! { i64 },
        FieldType::F64 => quote! { f64 },
        FieldType::Bool => quote! { bool },
        FieldType::String => quote! { String },
        FieldType::Uuid => quote! { uuid::Uuid },
        FieldType::Timestamp => quote! { i64 }, // Unix timestamp
        FieldType::Char(size) => {
            let size_lit = proc_macro2::Literal::usize_unsuffixed(*size);
            quote! { [u8; #size_lit] }
        }
        FieldType::FixedArray(inner, count) => {
            let inner_type = map_field_type_to_rust_tokens(inner, for_response);
            let count_lit = proc_macro2::Literal::usize_unsuffixed(*count);
            quote! { [#inner_type; #count_lit] }
        }
        FieldType::StructType(name) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            quote! { #ident }
        }
        FieldType::OptionalStructType(name) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            quote! { Option<#ident> }
        }
        FieldType::Relation(rel_type) => match rel_type {
            RelationType::RequiredReference(_) => quote! { uuid::Uuid },
            RelationType::OptionalReference(_) => quote! { Option<uuid::Uuid> },
            _ => quote! { () }, // Virtual fields
        },
        FieldType::Component(_) => quote! { () }, // Component references are virtual
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{IndexType, RelationType};

    fn create_test_field(name: &str, field_type: FieldType, is_computed: bool) -> Field {
        Field {
            name: name.to_string(),
            field_type,
            auto_generate: false,
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
    fn test_is_virtual_field() {
        let virtual_field = create_test_field(
            "posts",
            FieldType::Relation(RelationType::OneToMany("Post".to_string())),
            false,
        );
        assert!(is_virtual_field(&virtual_field));

        let stored_field = create_test_field("name", FieldType::String, false);
        assert!(!is_virtual_field(&stored_field));

        let reference_field = create_test_field(
            "author",
            FieldType::Relation(RelationType::RequiredReference("User".to_string())),
            false,
        );
        assert!(!is_virtual_field(&reference_field));
    }

    #[test]
    fn test_is_optional_field() {
        let optional_field = create_test_field(
            "address",
            FieldType::OptionalStructType("Address".to_string()),
            false,
        );
        assert!(is_optional_field(&optional_field));

        let required_field = create_test_field("name", FieldType::String, false);
        assert!(!is_optional_field(&required_field));
    }

    #[test]
    fn test_id_field() {
        let model = Model {
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
                create_test_field("name", FieldType::String, false),
            ],
            composite_indexes: vec![],
            soft_delete: false,
        };

        let id = id_field(&model);
        assert!(id.is_some());
        assert_eq!(id.unwrap().name, "id");
    }

    #[test]
    fn test_computed_fields() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![
                create_test_field("name", FieldType::String, false),
                create_test_field("full_name", FieldType::String, true),
                create_test_field("email", FieldType::String, false),
                create_test_field("display_name", FieldType::String, true),
            ],
            composite_indexes: vec![],
            soft_delete: false,
        };

        let computed = computed_fields(&model);
        assert_eq!(computed.len(), 2);
        assert_eq!(computed[0].name, "full_name");
        assert_eq!(computed[1].name, "display_name");
    }

    #[test]
    fn test_relation_field_name() {
        let ref_field = create_test_field(
            "author",
            FieldType::Relation(RelationType::RequiredReference("User".to_string())),
            false,
        );
        assert_eq!(relation_field_name(&ref_field), "author_id");

        let regular_field = create_test_field("name", FieldType::String, false);
        assert_eq!(relation_field_name(&regular_field), "name");
    }

    #[test]
    fn test_map_field_type_to_rust_tokens() {
        let tokens = map_field_type_to_rust_tokens(&FieldType::String, false);
        assert_eq!(tokens.to_string(), "String");

        let tokens = map_field_type_to_rust_tokens(&FieldType::Uuid, false);
        assert_eq!(tokens.to_string(), "uuid :: Uuid");

        let tokens = map_field_type_to_rust_tokens(&FieldType::U32, false);
        assert_eq!(tokens.to_string(), "u32");
    }
}
