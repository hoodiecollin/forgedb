//! API handlers generation using quote/syn
//!
//! Generates handler functions for CRUD operations using proper Rust code generation.

use crate::ast::Model;
use crate::codegen::{ir::IrModel, naming, GeneratedFile};
use proc_macro2::TokenStream;
use quote::quote;

/// Generate handler functions for a model using quote/syn
pub fn generate_handlers(model: &Model) -> GeneratedFile {
    let ir_model = IrModel::from_ast(model.clone());
    let model_lower = model.name.to_lowercase();
    let model_lower_ident = syn::Ident::new(&model_lower, proc_macro2::Span::call_site());
    let model_name_ident = syn::Ident::new(&model.name, proc_macro2::Span::call_site());
    let plural = naming::pluralize(&model_lower);
    
    // Generate individual handler functions
    let list_handler = generate_list_handler(&ir_model, &model_lower_ident, &model_name_ident);
    let get_handler = generate_get_handler(&ir_model, &model_lower_ident, &model_name_ident);
    let create_handler = generate_create_handler(&ir_model, &model_lower_ident, &model_name_ident);
    let update_handler = generate_update_handler(&ir_model, &model_lower_ident, &model_name_ident);
    let delete_handler = generate_delete_handler(&ir_model, &model_lower_ident, &model_name_ident);
    
    // Combine all handlers with imports
    let tokens = quote! {
        use axum::{
            extract::{Path, Query, State},
            http::StatusCode,
            response::{IntoResponse, Json},
        };
        use serde_json::json;
        use std::sync::Arc;
        use uuid::Uuid;
        
        use forgedb_query_params::QueryParams;
        
        #list_handler
        
        #get_handler
        
        #create_handler
        
        #update_handler
        
        #delete_handler
    };
    
    // Pretty-print the generated code
    let syntax_tree = syn::parse2(tokens).expect("Failed to parse generated tokens");
    let mut content = prettyplease::unparse(&syntax_tree);
    
    // Add the types module use statement at the top after the imports
    // (prettyplease doesn't handle super::module_name::* well in quote!)
    let types_use = format!("use super::{}_types::*;\n", model_lower);
    if let Some(pos) = content.find("use forgedb_query_params::QueryParams;") {
        let insert_pos = pos + "use forgedb_query_params::QueryParams;".len() + 1;
        content.insert_str(insert_pos, &types_use);
    }
    
    GeneratedFile {
        path: format!("generated/api/{}_handlers.rs", model_lower),
        content,
    }
}

/// Generate list handler with standardized response shape
fn generate_list_handler(
    model: &IrModel,
    model_lower: &syn::Ident,
    model_name: &syn::Ident,
) -> TokenStream {
    let doc_comment = format!("List all {}", naming::pluralize(&model.name.to_lowercase()));
    let list_fn_name = syn::Ident::new(&format!("list_{}", model_lower), proc_macro2::Span::call_site());
    
    quote! {
        #[doc = #doc_comment]
        pub async fn #list_fn_name(
            Query(params): Query<QueryParams>,
        ) -> impl IntoResponse {
            // TODO: Implement list logic with storage
            // Apply filters from params.filters
            // Apply sort from params.sort
            // Apply pagination from params.pagination
            Json(json!({
                "data": [],
                "total": 0,
                "limit": params.pagination.as_ref().map(|p| p.limit).unwrap_or(100),
                "offset": params.pagination.as_ref().map(|p| p.offset).unwrap_or(0)
            }))
        }
    }
}

/// Generate get by ID handler
fn generate_get_handler(
    model: &IrModel,
    model_lower: &syn::Ident,
    model_name: &syn::Ident,
) -> TokenStream {
    let doc_comment = format!("Get {} by ID", model_name);
    let get_fn_name = syn::Ident::new(&format!("get_{}", model_lower), proc_macro2::Span::call_site());
    
    quote! {
        #[doc = #doc_comment]
        pub async fn #get_fn_name(
            Path(id): Path<Uuid>,
        ) -> impl IntoResponse {
            // TODO: Implement get logic with storage
            (StatusCode::NOT_FOUND, Json(json!({
                "error": "Not found"
            })))
        }
    }
}

/// Generate create handler
fn generate_create_handler(
    model: &IrModel,
    model_lower: &syn::Ident,
    model_name: &syn::Ident,
) -> TokenStream {
    let doc_comment = format!("Create a new {}", model_name);
    let create_fn_name = syn::Ident::new(&format!("create_{}", model_lower), proc_macro2::Span::call_site());
    let request_type = syn::Ident::new(&format!("Create{}Request", model_name), proc_macro2::Span::call_site());
    
    quote! {
        #[doc = #doc_comment]
        pub async fn #create_fn_name(
            Json(req): Json<#request_type>,
        ) -> impl IntoResponse {
            // TODO: Implement create logic with storage
            // Validate request with forgedb_validation
            // Call storage.insert()
            (StatusCode::CREATED, Json(json!({
                "id": Uuid::new_v4()
            })))
        }
    }
}

/// Generate update handler
fn generate_update_handler(
    model: &IrModel,
    model_lower: &syn::Ident,
    model_name: &syn::Ident,
) -> TokenStream {
    let doc_comment = format!("Update an existing {}", model_name);
    let update_fn_name = syn::Ident::new(&format!("update_{}", model_lower), proc_macro2::Span::call_site());
    let request_type = syn::Ident::new(&format!("Update{}Request", model_name), proc_macro2::Span::call_site());
    
    quote! {
        #[doc = #doc_comment]
        pub async fn #update_fn_name(
            Path(id): Path<Uuid>,
            Json(req): Json<#request_type>,
        ) -> impl IntoResponse {
            // TODO: Implement update logic with storage
            (StatusCode::OK, Json(json!({
                "id": id
            })))
        }
    }
}

/// Generate delete handler
fn generate_delete_handler(
    model: &IrModel,
    model_lower: &syn::Ident,
    model_name: &syn::Ident,
) -> TokenStream {
    let doc_comment = format!("Delete a {}", model_name);
    let delete_fn_name = syn::Ident::new(&format!("delete_{}", model_lower), proc_macro2::Span::call_site());
    
    quote! {
        #[doc = #doc_comment]
        pub async fn #delete_fn_name(
            Path(id): Path<Uuid>,
        ) -> impl IntoResponse {
            // TODO: Implement delete logic with storage
            StatusCode::NO_CONTENT
        }
    }
}
