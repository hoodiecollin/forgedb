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

            // Bring the generated models + `Database` (defined in the sibling
            // `database` module and re-exported by the parent) into scope so the
            // `#[utoipa::path]` attributes and `OpenApi` derive can name them
            // unqualified, matching the `super::`-qualified handler signatures.
            use super::*;

            use axum::{
                extract::{Path, Query, State},
                extract::ws::{Message, WebSocket, WebSocketUpgrade},
                http::StatusCode,
                response::{Json, Response},
                routing::{get, post},
                Router,
            };
            use forgedb_types::Uuid;
            use serde_json::json;
            use std::collections::HashMap;
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

        // Generate the change-feed WebSocket subscription handler + per-model
        // filter for each model (#62 Direction A).
        for model in &schema.models {
            tokens.extend(Self::generate_subscription(model));
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

    /// Rust type a model's primary key parses into (mirrors `RustGenerator`'s
    /// identity type).  UUID PKs parse as `Uuid`; integer PKs as `u64` / `u32` /
    /// `i64` / `i32` so the generated `get` handler passes the right key type to
    /// storage.
    fn id_parse_type(model: &forgedb_parser::Model) -> TokenStream {
        match model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
        {
            Some(f) => match &f.field_type {
                forgedb_parser::FieldType::U32 => quote! { u32 },
                forgedb_parser::FieldType::U64 => quote! { u64 },
                forgedb_parser::FieldType::I32 => quote! { i32 },
                forgedb_parser::FieldType::I64 => quote! { i64 },
                forgedb_parser::FieldType::Uuid => quote! { Uuid },
                _ => quote! { Uuid },
            },
            None => quote! { Uuid },
        }
    }

    /// Generate handler functions for a model
    fn generate_handlers(model: &forgedb_parser::Model) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let id_type = Self::id_parse_type(model);
        let storage_field = format_ident!("{}", Self::to_snake_case(&model.name));
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
                State(_db): State<Arc<RwLock<super::Database>>>
            ) -> (StatusCode, Json<serde_json::Value>) {
                (StatusCode::OK, Json(json!({ "data": [] })))
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
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" }))),
                };
                let db = db.read().await;
                match db.#storage_field.get(key) {
                    Some(record) => {
                        let data = serde_json::to_value(&record).unwrap_or(json!(null));
                        (StatusCode::OK, Json(data))
                    }
                    None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
                }
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
                Json(payload): Json<serde_json::Value>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let record = match serde_json::from_value::<super::#model_name>(payload) {
                    Ok(record) => record,
                    Err(_) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "invalid payload" }))),
                };
                let mut db = db.write().await;
                let id = db.#storage_field.insert(record);
                (StatusCode::CREATED, Json(json!({ "id": id.to_string() })))
            }
        };

        Ok(tokens)
    }

    /// Whether a field is a JSON scalar a subscription filter can match on.
    /// Relations/components have no scalar JSON value; structs/arrays serialize to
    /// composites that a `?field=value` param would never sensibly match, so both
    /// are excluded from the generated per-model filter.
    fn is_filterable_field(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64
            | FieldType::Bool
            | FieldType::Uuid
            | FieldType::Timestamp
            | FieldType::String
            | FieldType::Char(_) => true,
            FieldType::Nullable(inner) => Self::is_filterable_field(inner),
            _ => false,
        }
    }

    /// Generate the change-feed WebSocket subscription handler + per-model filter
    /// for a model (#62 Direction A).  The handler subscribes to the shared feed,
    /// keeps only this model's `Inserted` signals, materializes the typed record
    /// from the broadcast row index, applies the generated filter, and streams a
    /// `<Model>Inserted` JSON event.  The substrate never inspects a field: model
    /// routing is by name and filtering is field-by-field in this generated code.
    fn generate_subscription(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let event_name = format_ident!("{}Inserted", model.name);
        let snake = Self::to_snake_case(&model.name);
        let storage_field = format_ident!("{}", snake);
        let subscribe_fn = format_ident!("subscribe_{}", snake);
        let handle_fn = format_ident!("handle_{}_subscription", snake);
        let filter_fn = format_ident!("{}_event_matches", snake);
        // The `&'static str` the generated `insert` emits for this model.
        let model_name_str = &model.name;

        // One equality check per declared scalar field — named explicitly so the
        // set of filterable keys is closed and per-model (never a generic scan).
        let field_checks: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| {
                let fname = &f.name;
                quote! {
                    if let Some(want) = params.get(#fname) {
                        let ok = obj.get(#fname).map(|v| match v {
                            serde_json::Value::String(s) => s == want,
                            other => other.to_string() == *want,
                        }).unwrap_or(false);
                        if !ok { return false; }
                    }
                }
            })
            .collect();

        let filter_doc = format!(
            "Per-model change-feed filter for `{}` (#62): narrow by exact-match \
             `?field=value` query params. Each declared scalar field is checked by \
             name in generated code; the substrate feed never inspects a field. An \
             empty param set matches everything; unknown keys are ignored.",
            model.name
        );
        let subscribe_doc = format!(
            "WebSocket subscription for `{}` inserts (#62 Direction A). Upgrades the \
             connection and streams a typed `{}Inserted` JSON event per insert, \
             optionally narrowed by `?field=value`.",
            model.name, model.name
        );

        quote! {
            #[doc = #filter_doc]
            fn #filter_fn(record: &super::#model_name, params: &HashMap<String, String>) -> bool {
                if params.is_empty() {
                    return true;
                }
                let value = match serde_json::to_value(record) {
                    Ok(v) => v,
                    Err(_) => return true,
                };
                let obj = match value.as_object() {
                    Some(o) => o,
                    None => return true,
                };
                #(#field_checks)*
                true
            }

            #[doc = #subscribe_doc]
            async fn #subscribe_fn(
                Query(params): Query<HashMap<String, String>>,
                ws: WebSocketUpgrade,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                ws.on_upgrade(move |socket| #handle_fn(socket, db, params))
            }

            async fn #handle_fn(
                mut socket: WebSocket,
                db: Arc<RwLock<super::Database>>,
                params: HashMap<String, String>,
            ) {
                // Subscribe without holding the DB lock across the stream: the feed
                // is Clone (shares the channel), so take a receiver and release.
                let mut rx = { db.read().await.changefeed.subscribe() };
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if event.model != #model_name_str {
                                continue;
                            }
                            if event.kind != forgedb_changefeed::ChangeKind::Inserted {
                                continue;
                            }
                            // Materialize the typed record from the row index (brief lock).
                            let record = { db.read().await.#storage_field.read_at(event.row_index) };
                            let Some(record) = record else { continue; };
                            if !#filter_fn(&record, &params) {
                                continue;
                            }
                            let payload = super::#event_name { #storage_field: record };
                            let Ok(text) = serde_json::to_string(&payload) else { continue; };
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                break; // client disconnected
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
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

                let subscribe_fn = format_ident!("subscribe_{}", Self::to_snake_case(&model.name));

                quote! {
                    .route(concat!("/api/", #route_path), get(#list_fn))
                    .route(concat!("/api/", #route_path), post(#create_fn))
                    .route(concat!("/api/", #route_path, "/{id}"), get(#get_fn))
                    // Change-feed WebSocket subscription (#62 Direction A).
                    .route(concat!("/subscribe/", #route_path), get(#subscribe_fn))
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
