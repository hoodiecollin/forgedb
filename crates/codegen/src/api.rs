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
                routing::{delete, get, post, put},
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

        // Generate the live-query WebSocket handler for each model (#62 Direction
        // B): reuses the per-model closed-set filter defined above, so it must be
        // emitted after the subscription handlers.
        for model in &schema.models {
            tokens.extend(Self::generate_live_query(model));
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

    /// The field identifier used as a model's identity (`id`, or the first
    /// auto-generate field).  Used by the live-query handler (#62 Direction B) to
    /// key result-set membership by id.  Falls back to `id`.
    fn id_field_ident(model: &forgedb_parser::Model) -> proc_macro2::Ident {
        match model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
        {
            Some(f) => format_ident!("{}", f.name),
            None => format_ident!("id"),
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
        let update_fn = format_ident!("update_{}", Self::to_snake_case(&model.name));
        let delete_fn = format_ident!("delete_{}", Self::to_snake_case(&model.name));

        let model_name_str = &model.name;
        let model_tag = &model.name;
        let list_summary = format!("List all {}", model.name);
        let get_summary = format!("Get {} by ID", model.name);
        let create_summary = format!("Create new {}", model.name);
        let update_summary = format!("Replace {} by ID", model.name);
        let delete_summary = format!("Delete {} by ID", model.name);

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

            #[utoipa::path(
                put,
                path = "/{id}",
                tag = #model_tag,
                params(
                    ("id" = String, Path, description = #model_name_str)
                ),
                request_body = #model_name,
                responses(
                    (status = 200, description = #update_summary, body = #model_name),
                    (status = 404, description = "Not found")
                )
            )]
            // Whole-record replace over the generated `update` (#66):
            // superseding-version append + id repoint, not a field-level patch.
            async fn #update_fn(
                Path(id): Path<String>,
                State(db): State<Arc<RwLock<super::Database>>>,
                Json(payload): Json<serde_json::Value>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" }))),
                };
                let record = match serde_json::from_value::<super::#model_name>(payload) {
                    Ok(record) => record,
                    Err(_) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "invalid payload" }))),
                };
                let mut db = db.write().await;
                if db.#storage_field.update(key, record) {
                    (StatusCode::OK, Json(json!({ "id": key.to_string() })))
                } else {
                    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))
                }
            }

            #[utoipa::path(
                delete,
                path = "/{id}",
                tag = #model_tag,
                params(
                    ("id" = String, Path, description = #model_name_str)
                ),
                responses(
                    (status = 204, description = #delete_summary),
                    (status = 404, description = "Not found")
                )
            )]
            // Tombstoned-version append over the generated `delete` (#66):
            // `get` then reads the row as absent; append-only storage is preserved.
            async fn #delete_fn(
                Path(id): Path<String>,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> StatusCode {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return StatusCode::BAD_REQUEST,
                };
                let mut db = db.write().await;
                if db.#storage_field.delete(key) {
                    StatusCode::NO_CONTENT
                } else {
                    StatusCode::NOT_FOUND
                }
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
        let inserted_name = format_ident!("{}Inserted", model.name);
        let updated_name = format_ident!("{}Updated", model.name);
        let deleted_name = format_ident!("{}Deleted", model.name);
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
            "WebSocket subscription for `{}` changes (#62 Direction A + #66). Upgrades \
             the connection and streams a typed `{}Inserted` / `{}Updated` / \
             `{}Deleted` JSON event per change, optionally narrowed by `?field=value`.",
            model.name, model.name, model.name, model.name
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
                            // Materialize the typed record from the row index (brief lock).
                            // Every model-change kind emits a materializable row: Inserted
                            // / Updated point at the new live version; Deleted carries the
                            // pre-delete row so the deleted record is still readable.
                            // `Linked` (M2M) is not a model-row change → skip.
                            let record = { db.read().await.#storage_field.read_at(event.row_index) };
                            let Some(record) = record else { continue; };
                            if !#filter_fn(&record, &params) {
                                continue;
                            }
                            let text = match event.kind {
                                forgedb_changefeed::ChangeKind::Inserted => {
                                    serde_json::to_string(&super::#inserted_name { #storage_field: record })
                                }
                                forgedb_changefeed::ChangeKind::Updated => {
                                    serde_json::to_string(&super::#updated_name { #storage_field: record })
                                }
                                forgedb_changefeed::ChangeKind::Deleted => {
                                    serde_json::to_string(&super::#deleted_name { #storage_field: record })
                                }
                                forgedb_changefeed::ChangeKind::Linked => continue,
                            };
                            let Ok(text) = text else { continue; };
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

    /// Generate the live-query WebSocket handler for a model (#62 Direction B).
    ///
    /// A stateful, removal-aware result-set subscription.  On connect it runs a
    /// **generated closed-set query** — `db.<model>.all()` filtered by the same
    /// generated per-model `<model>_event_matches` closed-set filter the REST
    /// `list` endpoint and #62-A use (NO second predicate parser: the filterable
    /// keys are the finite declared-scalar set, exact-match by name) — sends an
    /// `Init` delta, and records membership as `id -> opaque hash`.  On every
    /// change to this model it re-runs that same generated query, diffs by id over
    /// the opaque hashes, and pushes typed `Added` / `Updated` / `Removed` deltas.
    ///
    /// Identity: the substrate feed is consulted **coarsely — only `event.model`**
    /// (never `row_index`/`kind`), so no logical-row identity is resolved through
    /// the substrate and no `ChangeEvent` widening is needed.  Re-evaluation runs
    /// only generated code; the diff/membership plumbing is opaque ids + opaque
    /// hashes.  This is "generated code re-executing generated code on a coarse
    /// signal," not a runtime predicate interpreter.
    ///
    /// Honest limits: O(rows) full re-run per matched event per connection (no
    /// coalescing/debounce yet); `Updated` detection uses full-record
    /// `serde_json` stringify comparison, inheriting #62-A's exact-match
    /// fragility for some float/bool encodings; single-process.
    fn generate_live_query(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let delta_name = format_ident!("{}LiveDelta", model.name);
        let snake = Self::to_snake_case(&model.name);
        let storage_field = format_ident!("{}", snake);
        let subscribe_fn = format_ident!("subscribe_live_{}", snake);
        let handle_fn = format_ident!("handle_{}_live_query", snake);
        // Reuse the EXACT generated closed-set filter emitted by
        // `generate_subscription` — do not define a second filtering path.
        let filter_fn = format_ident!("{}_event_matches", snake);
        let id_field = Self::id_field_ident(model);
        let id_type = Self::id_parse_type(model);
        let model_name_str = &model.name;

        let subscribe_doc = format!(
            "Live-query WebSocket subscription for `{}` (#62 Direction B). Runs the \
             generated closed-set query `all()` + `{}_event_matches` (narrow with \
             `?field=value`), streams an initial `{}LiveDelta::Init`, then pushes \
             removal-aware `Added` / `Updated` / `Removed` deltas as the matching \
             set changes.",
            model.name, snake, model.name
        );

        quote! {
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
                // Coarse change signal: take a feed receiver (Clone shares the
                // channel), then release the DB lock.  We consult only event.model.
                let mut rx = { db.read().await.changefeed.subscribe() };

                // Result-set membership: id -> opaque hash of the record in the set.
                let mut members: HashMap<#id_type, String> = HashMap::new();

                // Initial matching set via the GENERATED closed-set query.
                {
                    let rows: Vec<super::#model_name> = {
                        let g = db.read().await;
                        g.#storage_field
                            .all()
                            .into_iter()
                            .filter(|r| #filter_fn(r, &params))
                            .collect()
                    };
                    for r in &rows {
                        members.insert(r.#id_field, serde_json::to_string(r).unwrap_or_default());
                    }
                    let init = super::#delta_name::Init { rows };
                    if let Ok(text) = serde_json::to_string(&init) {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            return; // client disconnected
                        }
                    }
                }

                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            // COARSE: any change to this model re-runs the query.
                            // Only the model NAME is read — never row_index/kind.
                            if event.model != #model_name_str {
                                continue;
                            }

                            // Re-run the SAME generated closed-set query.
                            let current: Vec<super::#model_name> = {
                                let g = db.read().await;
                                g.#storage_field
                                    .all()
                                    .into_iter()
                                    .filter(|r| #filter_fn(r, &params))
                                    .collect()
                            };

                            // Diff by id over opaque hashes → removal-aware deltas.
                            let mut next: HashMap<#id_type, String> = HashMap::new();
                            let mut deltas: Vec<super::#delta_name> = Vec::new();
                            for r in current {
                                let id = r.#id_field;
                                let hash = serde_json::to_string(&r).unwrap_or_default();
                                match members.get(&id) {
                                    None => deltas.push(super::#delta_name::Added { row: r.clone() }),
                                    Some(prev) if *prev != hash => {
                                        deltas.push(super::#delta_name::Updated { row: r.clone() })
                                    }
                                    _ => {}
                                }
                                next.insert(id, hash);
                            }
                            for id in members.keys() {
                                if !next.contains_key(id) {
                                    deltas.push(super::#delta_name::Removed { id: *id });
                                }
                            }
                            members = next;

                            for d in deltas {
                                let Ok(text) = serde_json::to_string(&d) else { continue; };
                                if socket.send(Message::Text(text.into())).await.is_err() {
                                    return; // client disconnected
                                }
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

        let update_handlers: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("update_{}", Self::to_snake_case(&model.name)))
            .collect();

        let delete_handlers: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("delete_{}", Self::to_snake_case(&model.name)))
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
                    #(#update_handlers,)*
                    #(#delete_handlers,)*
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
                let update_fn = format_ident!("update_{}", Self::to_snake_case(&model.name));
                let delete_fn = format_ident!("delete_{}", Self::to_snake_case(&model.name));

                let subscribe_fn = format_ident!("subscribe_{}", Self::to_snake_case(&model.name));
                let live_query_fn =
                    format_ident!("subscribe_live_{}", Self::to_snake_case(&model.name));

                quote! {
                    .route(concat!("/api/", #route_path), get(#list_fn))
                    .route(concat!("/api/", #route_path), post(#create_fn))
                    // GET/PUT/DELETE by id (#69): PUT = whole-record replace, DELETE = tombstone.
                    .route(
                        concat!("/api/", #route_path, "/{id}"),
                        get(#get_fn).put(#update_fn).delete(#delete_fn),
                    )
                    // Change-feed WebSocket subscription (#62 Direction A).
                    .route(concat!("/subscribe/", #route_path), get(#subscribe_fn))
                    // Live-query WebSocket subscription (#62 Direction B).
                    .route(concat!("/live-query/", #route_path), get(#live_query_fn))
                }
            })
            .collect();

        let tokens = quote! {
            /// Create the API router with all endpoints (no auth).
            pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {
                Router::new()
                    #(#routes)*
                    .with_state(db)
            }

            /// Create the API router with the tenant-auth guard layered over every
            /// route (#59).  Each request must carry a bearer JWT whose configured
            /// tenant claim equals this process's tenant — the `forgedb-auth`
            /// substrate verifies the signature (asymmetric, JWKS/static key,
            /// algorithm-pinned) and cross-checks the tenant, rejecting with 401
            /// (auth failure) or 403 (wrong tenant) before any handler runs; on
            /// success the verified `forgedb_auth::Principal` is injected into
            /// request extensions.  `auth` is built from deployment config
            /// (`forgedb.toml` / env), never from the `.forge` schema — the guard
            /// is a signed-string cross-check, not a schema-reading policy engine.
            ///
            /// The guard also covers the WS `/subscribe` and `/live-query` routes;
            /// WS clients must send the token in the `Authorization` header (clients
            /// that cannot set WS headers are a documented limitation).
            pub fn create_router_with_auth(
                db: Arc<RwLock<super::Database>>,
                auth: Arc<forgedb_auth::Authenticator>,
            ) -> Router {
                create_router(db).layer(axum::middleware::from_fn_with_state(
                    auth,
                    forgedb_auth::axum_mw::require_tenant,
                ))
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
