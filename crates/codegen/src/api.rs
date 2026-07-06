//! REST API server code generator

use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// API code generator
pub struct ApiGenerator;

impl ApiGenerator {
    /// Generate REST API server implementation from schema
    ///
    /// # Arguments
    ///
    /// * `schema` - Parsed schema AST
    ///
    /// # Returns
    ///
    /// Generated API code as a string
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let code = Self::generate_code(schema)?;

        Ok(GeneratedCode {
            code,
            description: format!("REST API server ({} models)", schema.models.len()),
        })
    }

    /// Generate API server code using quote!
    fn generate_code(schema: &Schema) -> Result<String> {
        let mut tokens = TokenStream::new();

        // File header comments
        let header = quote! {
            //! Generated API server by ForgeDB
            //! DO NOT EDIT - This file is auto-generated
        };
        tokens.extend(header);

        // Imports
        let imports = quote! {
            #![allow(dead_code, unused_imports)]

            use axum::{
                extract::{Path, State},
                http::StatusCode,
                response::Json,
                routing::{get, post},
                Router,
            };
            use serde_json::json;
            use std::sync::Arc;
            use tokio::sync::RwLock;
            use utoipa::OpenApi;
            use utoipa_axum::router::OpenApiRouter;
            use utoipa_axum::routes;
        };
        tokens.extend(imports);

        // Generate handler functions for each model
        for model in &schema.models {
            let handler_tokens = Self::generate_handlers(model)?;
            tokens.extend(handler_tokens);
        }

        // Generate OpenAPI doc struct
        let openapi_tokens = Self::generate_openapi_doc(schema)?;
        tokens.extend(openapi_tokens);

        // Generate router function
        let router_tokens = Self::generate_router(schema)?;
        tokens.extend(router_tokens);

        // Parse and format with prettyplease
        let syntax_tree = syn::parse_file(&tokens.to_string())
            .map_err(|e| crate::CodegenError::GenerationFailed(format!("Failed to parse generated code: {}", e)))?;

        Ok(prettyplease::unparse(&syntax_tree))
    }

    /// Generate handler functions for a model
    fn generate_handlers(model: &forgedb_parser::Model) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let list_fn = format_ident!("list_{}", Self::to_snake_case(&model.name));
        let get_fn = format_ident!("get_{}", Self::to_snake_case(&model.name));
        let create_fn = format_ident!("create_{}", Self::to_snake_case(&model.name));

        let model_name_str = &model.name;
        let model_tag = &model.name;
        let list_summary = format!("List all {}", model.name);
        let get_summary = format!("Get {} by ID", model.name);
        let create_summary = format!("Create new {}", model.name);

        let tokens = quote! {
            #[utoipa::path(
                get,
                path = "",
                tag = #model_tag,
                responses(
                    (status = 200, description = #list_summary, body = Vec<#model_name>)
                )
            )]
            async fn #list_fn(
                State(db): State<Arc<RwLock<super::Database>>>
            ) -> Json<serde_json::Value> {
                // TODO: Implement list
                Json(json!({ "data": [] }))
            }

            #[utoipa::path(
                get,
                path = "/{id}",
                tag = #model_tag,
                params(
                    ("id" = String, Path, description = #model_name_str)
                ),
                responses(
                    (status = 200, description = #get_summary, body = #model_name),
                    (status = 404, description = "Not found")
                )
            )]
            async fn #get_fn(
                Path(id): Path<String>,
                State(db): State<Arc<RwLock<super::Database>>>
            ) -> Json<serde_json::Value> {
                // TODO: Implement get
                Json(json!({ "data": null }))
            }

            #[utoipa::path(
                post,
                path = "",
                tag = #model_tag,
                request_body = #model_name,
                responses(
                    (status = 201, description = #create_summary, body = #model_name)
                )
            )]
            async fn #create_fn(
                State(db): State<Arc<RwLock<super::Database>>>,
                Json(payload): Json<serde_json::Value>
            ) -> Json<serde_json::Value> {
                // TODO: Implement create
                Json(json!({ "data": null }))
            }
        };

        Ok(tokens)
    }

    /// Generate OpenAPI documentation struct
    fn generate_openapi_doc(schema: &Schema) -> Result<TokenStream> {
        // Collect all handler functions
        let list_handlers: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("list_{}", Self::to_snake_case(&model.name)))
            .collect();

        let get_handlers: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("get_{}", Self::to_snake_case(&model.name)))
            .collect();

        let create_handlers: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("create_{}", Self::to_snake_case(&model.name)))
            .collect();

        // Collect all model schemas
        let model_schemas: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("{}", model.name))
            .collect();

        let tokens = quote! {
            #[derive(OpenApi)]
            #[openapi(
                paths(
                    #(#list_handlers,)*
                    #(#get_handlers,)*
                    #(#create_handlers,)*
                ),
                components(
                    schemas(#(#model_schemas,)*)
                ),
                tags(
                    #((name = stringify!(#model_schemas), description = concat!(stringify!(#model_schemas), " operations"))),*
                )
            )]
            pub struct ApiDoc;

            /// Get OpenAPI specification as JSON
            pub fn openapi_json() -> String {
                ApiDoc::openapi().to_json().unwrap()
            }
        };

        Ok(tokens)
    }

    /// Generate router function
    fn generate_router(schema: &Schema) -> Result<TokenStream> {
        // Generate route registrations
        let routes: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let route_path = Self::to_kebab_case(&model.name);
                let list_fn = format_ident!("list_{}", Self::to_snake_case(&model.name));
                let get_fn = format_ident!("get_{}", Self::to_snake_case(&model.name));
                let create_fn = format_ident!("create_{}", Self::to_snake_case(&model.name));

                quote! {
                    .route(concat!("/api/", #route_path), get(#list_fn))
                    .route(concat!("/api/", #route_path), post(#create_fn))
                    .route(concat!("/api/", #route_path, "/{id}"), get(#get_fn))
                }
            })
            .collect();

        let tokens = quote! {
            /// Create the API router with all endpoints
            pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {
                Router::new()
                    #(#routes)*
                    .with_state(db)
            }
        };

        Ok(tokens)
    }

    /// Convert PascalCase to snake_case
    fn to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }

    /// Convert PascalCase to kebab-case
    fn to_kebab_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('-');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }
}
