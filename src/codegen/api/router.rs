//! API router generation using quote/syn
//!
//! Generates router setup with all CRUD endpoints using proper Rust code generation.

use crate::ast::Schema;
use crate::codegen::{naming, GeneratedFile};
use proc_macro2::TokenStream;
use quote::quote;

/// Generate router setup using quote/syn
pub fn generate_router(schema: &Schema) -> GeneratedFile {
    // Generate handler module imports
    let handler_imports: Vec<TokenStream> = schema
        .models
        .iter()
        .map(|model| {
            let model_lower = model.name.to_lowercase();
            let module_name = syn::Ident::new(
                &format!("{}_handlers", model_lower),
                proc_macro2::Span::call_site(),
            );
            quote! {
                use super::#module_name;
            }
        })
        .collect();

    // Generate route registrations for all models
    let routes: Vec<TokenStream> = schema
        .models
        .iter()
        .map(|model| {
            let model_lower = model.name.to_lowercase();
            let plural = naming::pluralize(&model_lower);
            
            let handlers_module = syn::Ident::new(
                &format!("{}_handlers", model_lower),
                proc_macro2::Span::call_site(),
            );
            let list_fn = syn::Ident::new(
                &format!("list_{}", model_lower),
                proc_macro2::Span::call_site(),
            );
            let create_fn = syn::Ident::new(
                &format!("create_{}", model_lower),
                proc_macro2::Span::call_site(),
            );
            let get_fn = syn::Ident::new(
                &format!("get_{}", model_lower),
                proc_macro2::Span::call_site(),
            );
            let update_fn = syn::Ident::new(
                &format!("update_{}", model_lower),
                proc_macro2::Span::call_site(),
            );
            let delete_fn = syn::Ident::new(
                &format!("delete_{}", model_lower),
                proc_macro2::Span::call_site(),
            );

            // Use Axum-style path parameters with braces
            let collection_path = format!("/api/{}", plural);
            let item_path = format!("/api/{}/:id", plural);

            quote! {
                .route(#collection_path, get(#handlers_module::#list_fn))
                .route(#collection_path, post(#handlers_module::#create_fn))
                .route(#item_path, get(#handlers_module::#get_fn))
                .route(#item_path, put(#handlers_module::#update_fn))
                .route(#item_path, delete(#handlers_module::#delete_fn))
            }
        })
        .collect();

    // Combine everything into the router function
    let tokens = quote! {
        use axum::{
            routing::{delete, get, post, put},
            Router,
        };

        #(#handler_imports)*

        /// Create the API router with all endpoints
        pub fn create_router() -> Router {
            Router::new()
                #(#routes)*
        }
    };

    // Pretty-print the generated code
    let syntax_tree = syn::parse2(tokens).expect("Failed to parse generated tokens");
    let content = prettyplease::unparse(&syntax_tree);

    GeneratedFile {
        path: "generated/api/router.rs".to_string(),
        content,
    }
}
