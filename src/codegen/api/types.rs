//! API types generation using quote/syn for Rust code generation
//!
//! Generates Request/Response types for API endpoints with proper Rust syntax.

use crate::ast::Model;
use crate::codegen::ir::IrModel;
use crate::codegen::semantics;
use crate::codegen::GeneratedFile;
use proc_macro2::TokenStream;
use quote::quote;

/// Generate API types for a model using quote/syn
pub fn generate_api_types(model: &Model) -> GeneratedFile {
    let ir_model = IrModel::from_ast(model.clone());
    let model_lower = model.name.to_lowercase();
    
    // Generate the three struct definitions
    let create_request = generate_create_request(&ir_model);
    let update_request = generate_update_request(&ir_model);
    let response = generate_response(&ir_model);
    
    // Combine all token streams
    let tokens = quote! {
        use serde::{Deserialize, Serialize};
        use uuid::Uuid;
        
        #create_request
        
        #update_request
        
        #response
    };
    
    // Pretty-print the generated code
    let syntax_tree = syn::parse2(tokens).expect("Failed to parse generated tokens");
    let content = prettyplease::unparse(&syntax_tree);
    
    GeneratedFile {
        path: format!("generated/api/{}_types.rs", model_lower),
        content,
    }
}

/// Generate CreateRequest struct
fn generate_create_request(model: &IrModel) -> TokenStream {
    let model_name = syn::Ident::new(&model.name, proc_macro2::Span::call_site());
    let struct_name = syn::Ident::new(
        &format!("Create{}Request", model.name),
        proc_macro2::Span::call_site(),
    );
    
    let fields: Vec<TokenStream> = model
        .create_request_fields()
        .iter()
        .map(|field| {
            let field_name = syn::Ident::new(&field.name, proc_macro2::Span::call_site());
            let field_type = semantics::map_field_type_to_rust_tokens(&field.field_type, false);
            quote! {
                pub #field_name: #field_type
            }
        })
        .collect();
    
    quote! {
        #[derive(Debug, Deserialize)]
        pub struct #struct_name {
            #(#fields),*
        }
    }
}

/// Generate UpdateRequest struct
fn generate_update_request(model: &IrModel) -> TokenStream {
    let model_name = syn::Ident::new(&model.name, proc_macro2::Span::call_site());
    let struct_name = syn::Ident::new(
        &format!("Update{}Request", model.name),
        proc_macro2::Span::call_site(),
    );
    
    let fields: Vec<TokenStream> = model
        .update_request_fields()
        .iter()
        .map(|field| {
            let field_name = syn::Ident::new(&field.name, proc_macro2::Span::call_site());
            let field_type = semantics::map_field_type_to_rust_tokens(&field.field_type, false);
            quote! {
                pub #field_name: Option<#field_type>
            }
        })
        .collect();
    
    quote! {
        #[derive(Debug, Deserialize)]
        pub struct #struct_name {
            #(#fields),*
        }
    }
}

/// Generate Response struct
fn generate_response(model: &IrModel) -> TokenStream {
    let model_name = syn::Ident::new(&model.name, proc_macro2::Span::call_site());
    let struct_name = syn::Ident::new(
        &format!("{}Response", model.name),
        proc_macro2::Span::call_site(),
    );
    
    let fields: Vec<TokenStream> = model
        .response_fields()
        .iter()
        .map(|field| {
            let field_name = syn::Ident::new(&field.name, proc_macro2::Span::call_site());
            let field_type = semantics::map_field_type_to_rust_tokens(&field.field_type, true);
            quote! {
                pub #field_name: #field_type
            }
        })
        .collect();
    
    quote! {
        #[derive(Debug, Serialize)]
        pub struct #struct_name {
            #(#fields),*
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Field, FieldType, IndexType};

    fn create_test_model() -> Model {
        Model {
            name: "User".to_string(),
            fields: vec![
                Field {
                    name: "id".to_string(),
                    field_type: FieldType::Uuid,
                    unique: false,
                    indexed: false,
                    auto_generate: true,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "email".to_string(),
                    field_type: FieldType::String,
                    unique: true,
                    indexed: true,
                    auto_generate: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    unique: false,
                    indexed: false,
                    auto_generate: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            soft_delete: false,
        }
    }

    #[test]
    fn test_generate_api_types() {
        let model = create_test_model();
        let file = generate_api_types(&model);

        assert_eq!(file.path, "generated/api/user_types.rs");
        assert!(file.content.contains("CreateUserRequest"));
        assert!(file.content.contains("UpdateUserRequest"));
        assert!(file.content.contains("UserResponse"));
        assert!(file.content.contains("pub email: String"));
        // CreateRequest shouldn't have auto-generated fields
        assert!(!file.content.contains("CreateUserRequest {\n    pub id:"));
        // But UserResponse should have all fields including id
        assert!(file.content.contains("pub id: uuid::Uuid"));
    }
}
