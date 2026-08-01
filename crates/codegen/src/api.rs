//! REST API server code generator

use crate::{GenConfig, GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

thread_local! {
    /// The generate-time runtime-behavior config (#126) active for the current
    /// `ApiGenerator::generate*` call. Mirrors `RustGenerator`'s thread-local
    /// (that one is module-private): each entry point sets it before emitting,
    /// and the config-dependent sites (pagination clamp #141, metrics gate #151)
    /// read it via `active_cfg()`. Safe under parallel tests — set per-thread.
    static ACTIVE_CONFIG: std::cell::Cell<GenConfig> = const { std::cell::Cell::new(GenConfig::DEFAULT) };
}

/// API code generator
pub struct ApiGenerator;

impl ApiGenerator {
    /// Generate REST API server implementation from schema, using the default
    /// generate-time config (`GenConfig::DEFAULT`). See `generate_with_config`
    /// for the #126 configurable-behavior knobs.
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Self::generate_with_config(schema, GenConfig::DEFAULT)
    }

    /// Generate the REST API server with an explicit generate-time config (#126).
    /// The config tailors the emitted pagination clamp (#141) and gates the
    /// `/metrics` route (#151, Tier A). `GenConfig::DEFAULT` reproduces the
    /// pre-#126 output byte-for-byte.
    pub fn generate_with_config(schema: &Schema, config: GenConfig) -> Result<GeneratedCode> {
        ACTIVE_CONFIG.with(|c| c.set(config));
        let code = Self::generate_code(schema)?;

        Ok(GeneratedCode {
            code,
            description: format!("REST API server ({} models)", schema.models.len()),
        })
    }

    /// The generate-time runtime-behavior config (#126) active for the current
    /// `generate*` call. Read at each config-dependent emission site.
    fn active_cfg() -> GenConfig {
        ACTIVE_CONFIG.with(|c| c.get())
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

        // Pagination clamp bounds (#141, epic #126): baked from
        // `[server].page_default_limit` / `page_max_limit`; defaults 50 / 1000
        // (byte-identical). The generated list handler clamps against these
        // rather than the substrate's fixed consts, so an app can tailor the page
        // size without a runtime schema. Schema-blind — same value for every app.
        let cfg = Self::active_cfg();
        let __page_default_limit = proc_macro2::Literal::usize_unsuffixed(cfg.page_default_limit);
        let __page_max_limit = proc_macro2::Literal::usize_unsuffixed(cfg.page_max_limit);
        tokens.extend(quote! {
            const PAGE_DEFAULT_LIMIT: usize = #__page_default_limit;
            const PAGE_MAX_LIMIT: usize = #__page_max_limit;
        });

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

        // Generate the durable replication WS endpoint (#82 Direction C): a single
        // schema-wide `/replicate` handler that streams the field-blind broker
        // frames to a resumable follower.  Emitted once (not per model) — the
        // broker carries one global offset across all models.
        tokens.extend(Self::generate_replication_handler());

        // Generate the operational endpoints — liveness / readiness / metrics
        // (Phase 5 observability).  Schema-agnostic transport glue: `/health`
        // and `/ready` are identical for every app; `/metrics` reports per-model
        // row counts, generated by naming each model's storage field.  These
        // handlers stay OUTSIDE the tenant-auth guard in the router (below) so
        // load balancers / k8s probes can reach them without a JWT.
        tokens.extend(Self::generate_ops_handlers(schema));

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
    /// Build the `?projection=<name>` REST support for a model (#113): a closed
    /// set of declared projection names only — no ad-hoc `?fields=` — so the wire
    /// carries a compile-time-declared name, never a runtime column list (PM
    /// constraint 6).  Returns `(get_query_param, get_block, list_block)`, all
    /// empty when the model declares no projections.  On the `get` path we route
    /// through the narrow `get_<name>` read (full server-side skip of unselected
    /// columns); on `list` we field-copy the already-filtered/sorted page to the
    /// projection struct (filter/sort need full rows, so only the wire shrinks).
    /// Returns `(get_block, list_block)` — both empty when the model declares no
    /// projections.  The `get` handler always owns its own `Query(params)`
    /// extractor (needed for `?as_of=` even without projections, #85), so this
    /// function no longer emits one.
    fn generate_projection_rest(
        model: &forgedb_parser::Model,
        storage_field: &proc_macro2::Ident,
        id_type: &TokenStream,
    ) -> (TokenStream, TokenStream) {
        if model.projections.is_empty() {
            return (quote! {}, quote! {});
        }

        let mut get_arms = Vec::new();
        let mut list_arms = Vec::new();
        for proj in &model.projections {
            let name = &proj.name;
            let get_fn = format_ident!("get_{}", proj.name);
            let proj_ident = format_ident!(
                "{}{}",
                model.name,
                crate::rust::RustGenerator::projection_pascal(&proj.name)
            );
            let fields = crate::rust::RustGenerator::projected_field_set(model, proj);
            let field_copies: Vec<_> = fields
                .iter()
                .map(|f| {
                    let fname = format_ident!("{}", f.name);
                    quote! { #fname: r.#fname.clone() }
                })
                .collect();

            get_arms.push(quote! {
                #name => match db.#storage_field.#get_fn(key) {
                    Some(r) => (StatusCode::OK, Json(serde_json::to_value(&r).unwrap_or(json!(null)))),
                    None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
                },
            });
            list_arms.push(quote! {
                #name => page
                    .iter()
                    .map(|r| serde_json::to_value(super::#proj_ident { #(#field_copies),* }).unwrap_or(json!(null)))
                    .collect(),
            });
        }

        let get_block = quote! {
            // #113: named-projection point read (closed set of declared names).
            if let Some(__proj) = params.get("projection") {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" }))),
                };
                let db = db.read().await;
                return match __proj.as_str() {
                    #(#get_arms)*
                    _ => (StatusCode::BAD_REQUEST, Json(json!({ "error": "unknown projection" }))),
                };
            }
        };
        let list_block = quote! {
            // #113: named-projection list (filter/sort/paginate on full rows,
            // then emit only the projection's columns for the page).
            if let Some(__proj) = params.get("projection") {
                let data: Vec<serde_json::Value> = match __proj.as_str() {
                    #(#list_arms)*
                    _ => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "unknown projection" }))),
                };
                return (StatusCode::OK, Json(json!({
                    "data": data,
                    "total": total,
                    "limit": qp.pagination.limit,
                    "offset": qp.pagination.offset,
                })));
            }
        };
        (get_block, list_block)
    }

    fn generate_handlers(model: &forgedb_parser::Model) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let id_type = Self::id_parse_type(model);
        let storage_field = format_ident!("{}", Self::to_snake_case(&model.name));
        let (proj_get_block, proj_list_block) =
            Self::generate_projection_rest(model, &storage_field, &id_type);
        let list_fn = format_ident!("list_{}", Self::to_snake_case(&model.name));
        let get_fn = format_ident!("get_{}", Self::to_snake_case(&model.name));
        let create_fn = format_ident!("create_{}", Self::to_snake_case(&model.name));
        let update_fn = format_ident!("update_{}", Self::to_snake_case(&model.name));
        let delete_fn = format_ident!("delete_{}", Self::to_snake_case(&model.name));

        let model_name_str = &model.name;
        let model_tag = &model.name;
        let list_summary = format!("List all {}", model.name);
        // #141: the OpenAPI description reflects the generate-time-baked page
        // bounds (`[server].page_default_limit` / `page_max_limit`), not the
        // substrate's fixed 50/1000.
        let __cfg = Self::active_cfg();
        let limit_param_desc = format!(
            "Max rows (clamped to [1, {}]; default {})",
            __cfg.page_max_limit, __cfg.page_default_limit
        );
        let get_summary = format!("Get {} by ID", model.name);
        let create_summary = format!("Create new {}", model.name);
        let update_summary = format!("Replace {} by ID", model.name);
        let delete_summary = format!("Delete {} by ID", model.name);

        let sort_fn = format_ident!("{}_apply_sort", Self::to_snake_case(&model.name));
        let filter_fn = format_ident!("{}_event_matches", Self::to_snake_case(&model.name));
        // #160: narrow scan filter/sort for the live list path (id-bearing models).
        let has_id = crate::rust::RustGenerator::identity_field(model).is_some();
        let id_field = Self::id_field_ident(model);
        let scan_matches_fn = format_ident!("__{}_scan_matches", Self::to_snake_case(&model.name));
        let scan_matches_ref_fn =
            format_ident!("__{}_scan_matches_ref", Self::to_snake_case(&model.name));
        let scan_sort_fn = format_ident!("__{}_scan_sort", Self::to_snake_case(&model.name));
        // The live list source+filter+sort+page-materialize.  For an id-bearing
        // model this is the #160 narrow path: filter/sort a scan record (only the
        // filterable/sortable columns), then full-materialize ONLY the page.  A
        // model with no id keeps the original full `all()` scan (it has no
        // `id_to_row`-driven narrow scan and cannot be mutated anyway).
        // #160 (C): index pushdown — if the filter names an eligible indexed field,
        // resolve candidates from that field's index (O(matches)) instead of
        // scanning every row; else scan all narrow rows.  A parse failure falls
        // back to the full scan (`unwrap_or_else`), so a match is never missed.
        //
        // #224: the full-scan arms filter on the BORROWED scan view
        // (`__scan_all_filtered`), so a row the filter rejects never allocates its
        // strings — previously every live row was decoded into owned `String`s and
        // then `retain`ed away.  The pushdown arm still filters owned rows: it
        // resolves O(matches) candidates through the index and reads them
        // individually (`__scan_row_at`), so there is no buffered span to borrow
        // from and nothing to save.
        let pushdown_fields = crate::rust::RustGenerator::scan_pushdown_fields(model);
        let scan_source = if pushdown_fields.is_empty() {
            quote! {
                db.#storage_field.__scan_all_filtered(|r| #scan_matches_ref_fn(r, &params))
            }
        } else {
            let branches = pushdown_fields.iter().map(|f| {
                let fname = &f.name;
                let scan_by = format_ident!("__scan_by_{}", f.name);
                quote! {
                    if let Some(__v) = params.get(#fname) {
                        match db.#storage_field.#scan_by(__v) {
                            Some(mut __c) => {
                                __c.retain(|r| #scan_matches_fn(r, &params));
                                __c
                            }
                            None => db.#storage_field
                                .__scan_all_filtered(|r| #scan_matches_ref_fn(r, &params)),
                        }
                    }
                }
            });
            quote! {
                #(#branches else)* {
                    db.#storage_field.__scan_all_filtered(|r| #scan_matches_ref_fn(r, &params))
                }
            }
        };
        let live_list_block = if has_id {
            quote! {
                let mut __scan_rows = #scan_source;
                #scan_sort_fn(&mut __scan_rows, &qp.sort);
                let total = __scan_rows.len();
                let __page_ids: Vec<_> = qp.pagination
                    .apply(&__scan_rows)
                    .iter()
                    .map(|r| r.#id_field)
                    .collect();
                // Full-materialize only the paginated page (or 404-skip a row that
                // was deleted between the scan and here — the read lock makes that
                // impossible, but `filter_map` is the honest primitive).
                let page: Vec<super::#model_name> = __page_ids
                    .iter()
                    .filter_map(|__id| db.#storage_field.get(*__id))
                    .collect();
            }
        } else {
            quote! {
                let mut rows: Vec<super::#model_name> = db.#storage_field.all()
                    .into_iter()
                    .filter(|r| #filter_fn(r, &params))
                    .collect();
                #sort_fn(&mut rows, &qp.sort);
                let total = rows.len();
                let page: Vec<super::#model_name> = qp.pagination.apply(&rows).to_vec();
            }
        };

        let tokens = quote! {
            #[utoipa::path(
                get,
                path = "",
                tag = #model_tag,
                params(
                    ("limit" = Option<usize>, Query, description = #limit_param_desc),
                    ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
                    ("sort" = Option<String>, Query, description = "Field to sort by"),
                    ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
                    ("as_of" = Option<usize>, Query, description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"),
                ),
                responses(
                    (status = 200, description = #list_summary, body = Vec<#model_name>)
                )
            )]
            // Real list endpoint (#90): fetch live rows, then filter / sort /
            // paginate.  `forgedb_query_params` is schema-agnostic substrate — it
            // only parses the query string into generic Sort / Pagination; every
            // field-aware step is generated per-model.  Filtering reuses the exact
            // closed-set `#filter_fn` the change-feed / live-query paths use (no
            // second predicate parser); sorting uses the generated `#sort_fn`
            // comparator; pagination is clamped by the substrate (MAX_LIMIT).
            async fn #list_fn(
                Query(params): Query<HashMap<String, String>>,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                // Parse the generic query params (sort/order/limit/offset); the
                // remaining `?field=value` pairs stay in `params` for the filter.
                let mut qp = forgedb_query_params::QueryParams::from_map(params.clone());
                // #141: re-derive the page limit against the generate-time-baked
                // default/max (`PAGE_DEFAULT_LIMIT`/`PAGE_MAX_LIMIT`) instead of the
                // substrate's fixed `DEFAULT_LIMIT`/`MAX_LIMIT`.  The raw `?limit`
                // survives in `params` (from_map got a clone), so we recompute it
                // here: omitted ⇒ the baked default; otherwise clamped to
                // `[1, PAGE_MAX_LIMIT]`.  Offset is left as the substrate parsed it.
                qp.pagination.limit = params
                    .get("limit")
                    .and_then(|__s| __s.parse::<usize>().ok())
                    .unwrap_or(PAGE_DEFAULT_LIMIT)
                    .clamp(1, PAGE_MAX_LIMIT);
                // #85: optional point-in-time read.  `as_of=<watermark>` is an
                // opaque row-count position (a `usize`, same class as
                // limit/offset) — present ⇒ read `all_at(&Snapshot::new(w))`
                // instead of the live `all()`; a non-numeric value is a 400 (never
                // silently fall back to live — a client asking for a snapshot and
                // getting live data is a correctness trap).  `as_of` is not a model
                // field, so the closed-set filter below ignores it.
                let __as_of: Option<usize> = match params.get("as_of") {
                    Some(__w) => match __w.parse::<usize>() {
                        Ok(__n) => Some(__n),
                        Err(_) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({ "error": "as_of must be a non-negative integer watermark" })),
                            );
                        }
                    },
                    None => None,
                };
                let db = db.read().await;
                // Materialize `page` (Vec<Model>) + `total`.  Two paths:
                //  - live (no `as_of`): the #160 narrow path — filter/sort a scan
                //    record (only filterable/sortable columns), full-materialize
                //    ONLY the paginated page.
                //  - `as_of` snapshot: the original full-record path over
                //    `all_at(&Snapshot)` (a rarer inspector read; #159 already made
                //    its newest-version resolution sub-linear).
                let (page, total) = match __as_of {
                    Some(__w) => {
                        let mut rows: Vec<super::#model_name> = db
                            .#storage_field
                            .all_at(&forgedb_storage::Snapshot::new(__w))
                            .into_iter()
                            .filter(|r| #filter_fn(r, &params))
                            .collect();
                        #sort_fn(&mut rows, &qp.sort);
                        let total = rows.len();
                        let page: Vec<super::#model_name> = qp.pagination.apply(&rows).to_vec();
                        (page, total)
                    }
                    None => {
                        #live_list_block
                        (page, total)
                    }
                };
                #proj_list_block
                let body = json!({
                    "data": page,
                    "total": total,
                    "limit": qp.pagination.limit,
                    "offset": qp.pagination.offset,
                });
                (StatusCode::OK, Json(body))
            }

            #[utoipa::path(
                get,
                path = "/{id}",
                tag = #model_tag,
                params(
                    ("id" = String, Path, description = #model_name_str),
                    ("as_of" = Option<usize>, Query, description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400")
                ),
                responses(
                    (status = 200, description = #get_summary, body = #model_name),
                    (status = 404, description = "Not found")
                )
            )]
            async fn #get_fn(
                Path(id): Path<String>,
                Query(params): Query<HashMap<String, String>>,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                // #113 projection takes precedence (a projected read is live-only);
                // `?as_of=` below applies to the full-record point read.
                #proj_get_block
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" }))),
                };
                // #85: optional point-in-time read (see the list handler note).
                let __as_of: Option<usize> = match params.get("as_of") {
                    Some(__w) => match __w.parse::<usize>() {
                        Ok(__n) => Some(__n),
                        Err(_) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({ "error": "as_of must be a non-negative integer watermark" })),
                            );
                        }
                    },
                    None => None,
                };
                let db = db.read().await;
                let __found = match __as_of {
                    Some(__w) => db.#storage_field.get_at(&forgedb_storage::Snapshot::new(__w), key),
                    None => db.#storage_field.get(key),
                };
                match __found {
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
                    (status = 201, description = #create_summary, body = #model_name),
                    (status = 409, description = "Integrity conflict (duplicate unique / dangling foreign key)"),
                    (status = 422, description = "Field constraint violation")
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
                // Route through the Database-level validated wrapper (#91): FK
                // existence + field constraints + `&unique`, mapped to 409/422.
                match db.#create_fn(record) {
                    Ok(id) => (StatusCode::CREATED, Json(json!({ "id": id.to_string() }))),
                    Err(e) => {
                        let status = StatusCode::from_u16(e.status_code())
                            .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
                        (status, Json(json!({ "error": e.to_string() })))
                    }
                }
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
                    (status = 404, description = "Not found"),
                    (status = 409, description = "Integrity conflict (duplicate unique / dangling foreign key)"),
                    (status = 422, description = "Field constraint violation")
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
                // Route through the Database-level validated wrapper (#91).
                match db.#update_fn(key, record) {
                    Ok(true) => (StatusCode::OK, Json(json!({ "id": key.to_string() }))),
                    Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
                    Err(e) => {
                        let status = StatusCode::from_u16(e.status_code())
                            .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
                        (status, Json(json!({ "error": e.to_string() })))
                    }
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
                    (status = 404, description = "Not found"),
                    (status = 409, description = "Referenced by children (on_delete=restrict)")
                )
            )]
            // Route through the Database-level `delete_<model>` wrapper (delete
            // semantics): each child FK's `@on_delete` policy (restrict/cascade/
            // set_null) is applied + M2M junctions unlinked, then the row is
            // tombstoned.  A `restrict` violation surfaces as 409.
            async fn #delete_fn(
                Path(id): Path<String>,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" }))),
                };
                let mut db = db.write().await;
                match db.#delete_fn(key) {
                    Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
                    Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
                    Err(e) => {
                        let status = StatusCode::from_u16(e.status_code())
                            .unwrap_or(StatusCode::CONFLICT);
                        (status, Json(json!({ "error": e.to_string() })))
                    }
                }
            }
        };

        let sort_tokens = Self::generate_list_sort(model);
        let scan_helpers = Self::generate_list_scan_helpers(model);
        Ok(quote! { #tokens #sort_tokens #scan_helpers })
    }

    /// Generate the per-model `<model>_apply_sort` comparator used by the list
    /// endpoint (#90).  Closed-set: a `match` over each declared scalar field by
    /// name selects a typed comparison (`Ord::cmp`, or `f64::partial_cmp` for
    /// floating-point fields); an unknown / relation `sort` field is a no-op.
    /// `forgedb_query_params::Sort` supplies only the field name + direction —
    /// the type-aware ordering is generated here, never interpreted at runtime.
    fn generate_list_sort(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let sort_fn = format_ident!("{}_apply_sort", Self::to_snake_case(&model.name));
        let arms = Self::list_sort_arms(model);

        quote! {
            fn #sort_fn(
                rows: &mut Vec<super::#model_name>,
                sort: &Option<forgedb_query_params::Sort>,
            ) {
                let Some(sort) = sort.as_ref() else { return; };
                match sort.field.as_str() {
                    #(#arms)*
                    _ => return,
                }
                if sort.is_descending() {
                    rows.reverse();
                }
            }
        }
    }

    /// The per-field `match`-arm comparators for the list sort (#90) — shared by
    /// the full-record `<model>_apply_sort` and the #160 narrow
    /// `__<model>_scan_sort`, so both order identically (each arm accesses
    /// `a.<field>`/`b.<field>`, which compiles against any struct carrying the
    /// filterable/sortable fields).
    fn list_sort_arms(model: &forgedb_parser::Model) -> Vec<TokenStream> {
        model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| {
                let fname = &f.name;
                let fident = format_ident!("{}", f.name);
                if Self::is_float_field(&f.field_type) {
                    // f64 (or nullable f64): only PartialOrd — fall back to Equal
                    // for NaN so the total order `sort_by` requires is well-defined.
                    quote! {
                        #fname => rows.sort_by(|a, b| {
                            a.#fident.partial_cmp(&b.#fident)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        }),
                    }
                } else {
                    // Every other filterable scalar is `Ord` (integers, bool,
                    // String, char(N), Uuid, Timestamp, and their nullable forms).
                    quote! {
                        #fname => rows.sort_by(|a, b| a.#fident.cmp(&b.#fident)),
                    }
                }
            })
            .collect()
    }

    /// #160: the narrow list helpers — `__<model>_scan_matches` and
    /// `__<model>_scan_sort` over the internal `<Model>ScanRow` (id +
    /// filterable/sortable columns).  Emitted from the SAME `generate_filter_check`
    /// per-field checks and `list_sort_arms` as the full-record `_event_matches` /
    /// `_apply_sort`, so filtering + ordering are byte-identical — only the operand
    /// type is narrower.  Lets the list endpoint filter/sort without decoding every
    /// column, then full-materialize only the paginated page.  Empty for a model
    /// with no id field (no list to optimize).
    fn generate_list_scan_helpers(model: &forgedb_parser::Model) -> TokenStream {
        if crate::rust::RustGenerator::identity_field(model).is_none() {
            return quote! {};
        }
        let scan_ident = format_ident!("{}ScanRow", model.name);
        let scan_ref_ident = format_ident!("{}ScanRef", model.name);
        let snake = Self::to_snake_case(&model.name);
        let scan_matches_fn = format_ident!("__{}_scan_matches", snake);
        let scan_matches_ref_fn = format_ident!("__{}_scan_matches_ref", snake);
        let scan_sort_fn = format_ident!("__{}_scan_sort", snake);
        let field_checks: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| Self::generate_filter_check(f, false))
            .collect();
        // #224: the borrowed-view filter comes from the SAME emitter, not a second
        // predicate — same param keys, same parse, same semantics.  Only the
        // nullable-string comparison differs, because `Option`'s `PartialEq` is
        // homogeneous (see `generate_filter_check`).
        let field_checks_ref: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| Self::generate_filter_check(f, true))
            .collect();
        let arms = Self::list_sort_arms(model);
        quote! {
            /// Narrow closed-set filter over the scan record (#160) — same per-field
            /// checks as `_event_matches`, only the operand type is narrower.
            fn #scan_matches_fn(record: &super::#scan_ident, params: &HashMap<String, String>) -> bool {
                if params.is_empty() {
                    return true;
                }
                #(#field_checks)*
                true
            }

            /// The same filter over the BORROWED scan view (#224), so a row can be
            /// accepted or rejected before its strings are ever copied out of the
            /// buffered column.  Emitted from the identical per-field checks as
            /// the owned filter above — one predicate, two operand views.
            fn #scan_matches_ref_fn(
                record: &super::#scan_ref_ident<'_>,
                params: &HashMap<String, String>,
            ) -> bool {
                if params.is_empty() {
                    return true;
                }
                #(#field_checks_ref)*
                true
            }

            /// Narrow list sort over the scan record (#160) — same arms as
            /// `_apply_sort`.
            fn #scan_sort_fn(
                rows: &mut Vec<super::#scan_ident>,
                sort: &Option<forgedb_query_params::Sort>,
            ) {
                let Some(sort) = sort.as_ref() else { return; };
                match sort.field.as_str() {
                    #(#arms)*
                    _ => return,
                }
                if sort.is_descending() {
                    rows.reverse();
                }
            }
        }
    }

    /// Whether a field's underlying scalar is `f64` (directly or nullable),
    /// which only implements `PartialOrd` — so generated sort must use
    /// `partial_cmp` rather than `Ord::cmp`.
    fn is_float_field(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::F64 => true,
            FieldType::Nullable(inner) => Self::is_float_field(inner),
            _ => false,
        }
    }

    /// Whether a field is a JSON scalar a subscription filter can match on.
    /// Relations/components have no scalar JSON value; structs/arrays serialize to
    /// composites that a `?field=value` param would never sensibly match, so both
    /// are excluded from the generated per-model filter.
    pub(crate) fn is_filterable_field(field_type: &forgedb_parser::FieldType) -> bool {
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
            // decimal is `Ord`, so it is filterable + sortable (sort uses the
            // `Ord::cmp` branch, not float `partial_cmp`).
            | FieldType::Decimal
            // enum derives `Ord`, so it is filterable + sortable via `Ord::cmp`
            // (sort orders by declaration order — the variant discriminant).
            | FieldType::Enum(_)
            | FieldType::Char(_) => true,
            FieldType::Nullable(inner) => Self::is_filterable_field(inner),
            _ => false,
        }
    }

    /// Split a field type into `(base, is_nullable)` — peeling a single
    /// `Nullable(..)` wrapper.  Used by the typed comparison generators (#84).
    fn peel_nullable(
        field_type: &forgedb_parser::FieldType,
    ) -> (&forgedb_parser::FieldType, bool) {
        match field_type {
            forgedb_parser::FieldType::Nullable(inner) => (inner.as_ref(), true),
            other => (other, false),
        }
    }

    /// Emit one **typed** equality check for a filterable field inside
    /// `<model>_event_matches` (#84).  Parses the string query param into the
    /// field's Rust type and compares it to the typed record field, so
    /// `?price=3` matches a stored `3.0` and bool/uuid/decimal/enum/timestamp
    /// values compare by value — not by the fragile `serde_json` stringify the
    /// old filter used (`3.0` != `"3"`).  An unparseable param matches nothing.
    ///
    /// The caller guarantees `field` is filterable (`is_filterable_field`); any
    /// other type yields an empty check.
    ///
    /// `borrowed` selects the operand view (#224).  Only ONE comparison actually
    /// differs: a **nullable string** on the borrowed view is `Option<&str>`, and
    /// `Option`'s `PartialEq` is homogeneous — there is no
    /// `Option<&str> == Option<String>` — so the parsed param has to be borrowed
    /// too (`Some(__w.as_str())`).  Everything else compiles against both views
    /// unchanged: non-nullable `&str == String` has a std heterogeneous impl, and
    /// every non-string field has the same type in both structs.
    fn generate_filter_check(field: &forgedb_parser::Field, borrowed: bool) -> TokenStream {
        use forgedb_parser::FieldType;
        let fname_str = &field.name;
        let fname = format_ident!("{}", field.name);
        let (base, nullable) = Self::peel_nullable(&field.field_type);

        // char(N) is a fixed `[u8; N]`: compare the param's bytes, zero-padded to
        // N (a param longer than N can never match).  Handled inline because it
        // parses into a buffer rather than a single `T`.
        if let FieldType::Char(n) = base {
            let cmp = if nullable {
                quote! { record.#fname == Some(__buf) }
            } else {
                quote! { record.#fname == __buf }
            };
            return quote! {
                if let Some(want) = params.get(#fname_str) {
                    let __wb = want.as_bytes();
                    let __ok = if __wb.len() > #n {
                        false
                    } else {
                        let mut __buf = [0u8; #n];
                        __buf[..__wb.len()].copy_from_slice(__wb);
                        #cmp
                    };
                    if !__ok { return false; }
                }
            };
        }

        // `<parse> -> Option<T>`; `None` means the param can't match this typed
        // field (unparseable / wrong shape).
        let parse: TokenStream = match base {
            FieldType::U32 => quote! { want.parse::<u32>().ok() },
            FieldType::U64 => quote! { want.parse::<u64>().ok() },
            FieldType::I32 => quote! { want.parse::<i32>().ok() },
            FieldType::I64 => quote! { want.parse::<i64>().ok() },
            FieldType::F64 => quote! { want.parse::<f64>().ok() },
            FieldType::Bool => quote! { want.parse::<bool>().ok() },
            FieldType::String => quote! { Some(want.clone()) },
            FieldType::Uuid => quote! { want.parse::<Uuid>().ok() },
            FieldType::Decimal => quote! { want.parse::<rust_decimal::Decimal>().ok() },
            FieldType::Timestamp => {
                quote! { want.parse::<i64>().ok().map(forgedb_types::Timestamp::from_seconds) }
            }
            FieldType::Enum(name) => {
                let en = format_ident!("{}", name);
                // Reuse the canonical variant-name <-> enum serde mapping so the
                // wire form matches REST/TS exactly (no second name table).
                quote! {
                    serde_json::from_value::<super::#en>(
                        serde_json::Value::String(want.clone())
                    ).ok()
                }
            }
            // Not filterable (caller guards); emit nothing.
            _ => return quote! {},
        };

        let cmp = if nullable {
            if borrowed && matches!(base, FieldType::String) {
                quote! { record.#fname == Some(__w.as_str()) }
            } else {
                quote! { record.#fname == Some(__w) }
            }
        } else {
            quote! { record.#fname == __w }
        };

        quote! {
            if let Some(want) = params.get(#fname_str) {
                let __ok = match #parse {
                    Some(__w) => #cmp,
                    None => false,
                };
                if !__ok { return false; }
            }
        }
    }

    /// Is this field compared for change-detection in the live-query diff (#84)?
    /// Everything the record actually stores/serializes — scalars, `json`,
    /// `decimal`, enums, `char(N)`, structs, and FK scalars — but NOT the virtual
    /// relation collections (one-to-many / many-to-many) or component refs, which
    /// map to `()` and carry no per-record value.
    fn is_comparable_field(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::{FieldType, RelationType};
        match field_type {
            FieldType::Relation(RelationType::RequiredReference(_))
            | FieldType::Relation(RelationType::OptionalReference(_)) => true,
            FieldType::Relation(_) | FieldType::Component(_) => false,
            FieldType::Nullable(inner) => Self::is_comparable_field(inner),
            _ => true,
        }
    }

    /// Generate `<model>_record_changed(a, b) -> bool` — a **typed, per-field**
    /// change detector for the live-query `Updated` diff (#84), replacing the old
    /// whole-record `serde_json` stringify comparison.  `f64` fields compare by
    /// `to_bits()` (deterministic — two `NaN`s are equal, so an unchanged record
    /// never reports a spurious update); every other stored field compares with
    /// `==`.  Returns `true` on the first differing field.
    fn generate_record_changed(model: &forgedb_parser::Model) -> TokenStream {
        use forgedb_parser::FieldType;
        let model_name = format_ident!("{}", model.name);
        let changed_fn = format_ident!("{}_record_changed", Self::to_snake_case(&model.name));

        let field_cmps: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_comparable_field(&f.field_type))
            .map(|f| {
                let fname = format_ident!("{}", f.name);
                let (base, nullable) = Self::peel_nullable(&f.field_type);
                match (base, nullable) {
                    (FieldType::F64, false) => quote! {
                        if a.#fname.to_bits() != b.#fname.to_bits() { return true; }
                    },
                    (FieldType::F64, true) => quote! {
                        if match (a.#fname, b.#fname) {
                            (Some(__x), Some(__y)) => __x.to_bits() != __y.to_bits(),
                            (None, None) => false,
                            _ => true,
                        } { return true; }
                    },
                    _ => quote! {
                        if a.#fname != b.#fname { return true; }
                    },
                }
            })
            .collect();

        quote! {
            /// Typed per-field change detector for the live-query `Updated` diff (#84).
            fn #changed_fn(a: &super::#model_name, b: &super::#model_name) -> bool {
                #(#field_cmps)*
                false
            }
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

        // One TYPED equality check per declared scalar field — named explicitly so
        // the set of filterable keys is closed and per-model (never a generic
        // scan).  Each check parses the string param into the field's Rust type
        // and compares typed values (#84), not fragile `serde_json` stringify.
        let field_checks: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| Self::generate_filter_check(f, false))
            .collect();

        // Typed per-field change detector for the live-query `Updated` diff (#84),
        // defined here (once per model) and reused by `generate_live_query`.
        let record_changed = Self::generate_record_changed(model);

        let filter_doc = format!(
            "Per-model change-feed filter for `{}` (#62): narrow by exact-match \
             `?field=value` query params. Each declared scalar field is checked by \
             name in generated code, parsing the param into the field's type and \
             comparing typed values (#84 — `?n=3` matches a stored `3.0`); the \
             substrate feed never inspects a field. An empty param set matches \
             everything; unknown keys are ignored.",
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
                #(#field_checks)*
                true
            }

            #record_changed

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

    /// Generate the durable replication WebSocket endpoint (#82 Direction C).
    ///
    /// One schema-wide handler (the broker carries a single global offset across
    /// all models).  A follower connects with `?after=<offset>` (its last applied
    /// offset, or omitted / `0` when cold) and receives a resumable stream of
    /// **field-blind binary frames** ([`PersistedEvent::to_wire`]): first the
    /// durably-retained frames it missed, then the live tail.  The handler NEVER
    /// decodes a frame — it forwards opaque `(model, row_index, kind, offset,
    /// bytes)` verbatim; typed materialization is the follower's generated code.
    ///
    /// Identity: this is Class-2 transport glue over the Class-1 broker.  Routing
    /// is by opaque model name and the apply order is the opaque global offset —
    /// no `match model_name { ... field ... }`, no per-model branch.  Sits behind
    /// the same `forgedb-auth` tenant guard as the CRUD/WS routes (it is added to
    /// `__data_routes`), so a follower receives only its own tenant's stream.
    fn generate_replication_handler() -> TokenStream {
        quote! {
            /// Upgrade to a replication stream.  `?after=<offset>` resumes from the
            /// follower's last applied offset (default `0` = cold / from the start
            /// of the retained log).  Tenant-scoped by the router's auth guard.
            async fn __replicate(
                Query(params): Query<HashMap<String, String>>,
                ws: WebSocketUpgrade,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                ws.on_upgrade(move |socket| __handle_replicate(socket, db, params))
            }

            async fn __handle_replicate(
                mut socket: WebSocket,
                db: Arc<RwLock<super::Database>>,
                params: HashMap<String, String>,
            ) {
                let after: u64 = params
                    .get("after")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                // Clone the shared broker handle out, releasing the DB read lock
                // immediately (the broker is an independent Arc<Mutex<..>>).
                let broker = { db.read().await.broker.clone() };
                let Some(broker) = broker else {
                    // Durable replication is not enabled on this process (e.g. a
                    // `Database::new()` standalone path); close cleanly.
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                };

                // Race-free resume: subscribe to the live tail AND read the durable
                // replay under ONE brief lock (single-writer, so no `record`
                // interleaves).  Everything > `boundary` is guaranteed to arrive live.
                let catch = match broker.lock() {
                    Ok(b) => b.catch_up_from(after, usize::MAX),
                    Err(_) => return, // poisoned lock: bail
                };
                let Ok(mut catch) = catch else { return };

                // 1. Replay the durably-retained frames the follower missed.
                for ev in &catch.replayed {
                    if socket
                        .send(Message::Binary(ev.to_wire().into()))
                        .await
                        .is_err()
                    {
                        return; // follower disconnected
                    }
                }

                // 2. Stream the live tail, skipping any frame already covered by the
                //    replay — idempotent by absolute offset (never by content).
                let boundary = catch.boundary;
                loop {
                    match catch.receiver.recv().await {
                        Ok(ev) => {
                            if ev.offset <= boundary {
                                continue;
                            }
                            if socket
                                .send(Message::Binary(ev.to_wire().into()))
                                .await
                                .is_err()
                            {
                                break; // follower disconnected
                            }
                        }
                        // The follower fell behind the live ring buffer.  Durable
                        // replay covers the gap, so close and let it reconnect with
                        // `?after=<last applied offset>` to resume losslessly.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let _ = socket.send(Message::Close(None)).await;
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    /// Generate the live-query WebSocket handler for a model (#62 Direction B).
    ///
    /// A stateful, removal-aware result-set subscription.  On connect it runs a
    /// **generated closed-set query** — the narrow `db.<model>.__scan_all()`
    /// filtered by the generated per-model `__<model>_scan_matches` closed-set
    /// filter (the SAME per-field checks as `<model>_event_matches` / the REST
    /// `list` endpoint, only the operand is the narrow scan row — NO second
    /// predicate parser), then full-materializes ONLY the matching rows (#160) —
    /// sends an `Init` delta, and records membership as `id -> typed record`.  On
    /// every change to this model it re-runs that same query, diffs by id, and
    /// pushes typed `Added` / `Updated` / `Removed` deltas.
    ///
    /// Identity: the substrate feed is consulted **coarsely — only `event.model`**
    /// (never `row_index`/`kind`), so no logical-row identity is resolved through
    /// the substrate and no `ChangeEvent` widening is needed.  Re-evaluation runs
    /// only generated code; the diff/membership plumbing is opaque ids + opaque
    /// hashes.  This is "generated code re-executing generated code on a coarse
    /// signal," not a runtime predicate interpreter.
    ///
    /// Honest limits: re-runs on every matched event per connection (no
    /// coalescing/debounce yet — #83); single-process.  #160 narrows the re-run —
    /// it scans only the filterable columns and full-materializes only the rows
    /// that match the filter (not every column of every row) — but a broad filter
    /// still materializes its whole matching set.  `Updated` detection uses a typed
    /// per-field comparison (`<model>_record_changed`, #84).
    fn generate_live_query(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let delta_name = format_ident!("{}LiveDelta", model.name);
        let snake = Self::to_snake_case(&model.name);
        let storage_field = format_ident!("{}", snake);
        let subscribe_fn = format_ident!("subscribe_live_{}", snake);
        let handle_fn = format_ident!("handle_{}_live_query", snake);
        // Typed per-field change detector (#84), also defined by
        // `generate_subscription` — reused here so there is one change-detection
        // body per model, never a second (stringify) path.
        let changed_fn = format_ident!("{}_record_changed", snake);
        // #160: the narrow closed-set filter over `<Model>ScanRow` (id +
        // filterable columns), co-emitted with the list path. The live-query
        // re-evaluation filters the CHEAP narrow scan and full-materializes only
        // the matching rows, instead of full-decoding every column of every row.
        // #224: it filters the BORROWED scan view, so a re-run rejects non-matching
        // rows before their strings are copied — the re-run happens on every change
        // to the model, so this is the hottest of the three call sites.
        let scan_matches_ref_fn = format_ident!("__{}_scan_matches_ref", snake);
        let id_field = Self::id_field_ident(model);
        let id_type = Self::id_parse_type(model);
        let model_name_str = &model.name;

        let subscribe_doc = format!(
            "Live-query WebSocket subscription for `{}` (#62 Direction B). Runs the \
             generated closed-set query (narrow `__scan_all` + `__{}_scan_matches`, \
             materializing only matches — #160), streams an initial \
             `{}LiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / \
             `Removed` deltas as the matching set changes.",
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

                // Result-set membership: id -> the typed record currently in the
                // set.  Change-detection compares these field-by-field (#84).
                let mut members: HashMap<#id_type, super::#model_name> = HashMap::new();

                // Initial matching set via the GENERATED closed-set query (#160:
                // filter the narrow scan, full-materialize only the matches).
                {
                    let rows: Vec<super::#model_name> = {
                        let g = db.read().await;
                        g.#storage_field
                            .__scan_all_filtered(|r| #scan_matches_ref_fn(r, &params))
                            .into_iter()
                            .filter_map(|r| g.#storage_field.get(r.#id_field))
                            .collect()
                    };
                    for r in &rows {
                        members.insert(r.#id_field, r.clone());
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

                            // Re-run the SAME generated closed-set query (#160:
                            // narrow scan + filter, full-materialize only matches).
                            let current: Vec<super::#model_name> = {
                                let g = db.read().await;
                                g.#storage_field
                                    .__scan_all_filtered(|r| #scan_matches_ref_fn(r, &params))
                                    .into_iter()
                                    .filter_map(|r| g.#storage_field.get(r.#id_field))
                                    .collect()
                            };

                            // Diff by id over the typed records → removal-aware
                            // deltas.  `Updated` fires only when a stored field
                            // actually changed (typed compare, #84 — no stringify).
                            let mut next: HashMap<#id_type, super::#model_name> = HashMap::new();
                            let mut deltas: Vec<super::#delta_name> = Vec::new();
                            for r in current {
                                let id = r.#id_field;
                                match members.get(&id) {
                                    None => deltas.push(super::#delta_name::Added { row: r.clone() }),
                                    Some(prev) if #changed_fn(prev, &r) => {
                                        deltas.push(super::#delta_name::Updated { row: r.clone() })
                                    }
                                    _ => {}
                                }
                                next.insert(id, r);
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

    /// Generate the operational endpoints (Phase 5 — observability):
    /// `/health` (liveness), `/ready` (readiness), `/metrics` (minimal JSON
    /// metrics).  Identity: `/health` and `/ready` are schema-agnostic and byte
    /// identical across apps; `/metrics` is generated per-schema only in that it
    /// names each model's storage field to report its `row_count()` — no schema
    /// is read at runtime.  All three are wired UNAUTHENTICATED in the router so
    /// infra probes/scrapers reach them without a tenant JWT (documented; a
    /// process serves one tenant, so `/metrics` leaks only that tenant's counts).
    fn generate_ops_handlers(schema: &Schema) -> TokenStream {
        // Per-model `"model": db.<field>.row_count()` entries — used by both
        // `/metrics` and the `/snapshot` watermark map, so kept unconditional.
        let metric_entries: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let name = &model.name;
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { #name: db.#field.row_count(), }
            })
            .collect();

        // #151 (Tier A): gate the `/metrics` handler on `[server].metrics`. When
        // off, neither the handler nor (below) its route is emitted; `/snapshot`
        // — which reuses `metric_entries` — is unaffected. `sum_fields` /
        // `model_count` are built INSIDE the branch so they aren't unused locals
        // when metrics is off.
        let metrics_handler = if Self::active_cfg().metrics {
            let sum_fields: Vec<_> = schema
                .models
                .iter()
                .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
                .collect();
            let model_count = schema.models.len();
            quote! {
                /// Minimal metrics (Phase 5): per-model live row counts + totals,
                /// as JSON.  Generated per-schema by naming each model's storage field;
                /// no schema is interpreted at runtime.
                async fn __metrics(
                    State(db): State<Arc<RwLock<super::Database>>>,
                ) -> (StatusCode, Json<serde_json::Value>) {
                    let db = db.read().await;
                    let per_model = json!({ #(#metric_entries)* });
                    let total_rows: usize = 0 #( + db.#sum_fields.row_count() )*;
                    let body = json!({
                        "model_count": #model_count,
                        "total_rows": total_rows,
                        "rows_per_model": per_model,
                    });
                    (StatusCode::OK, Json(body))
                }
            }
        } else {
            quote! {}
        };

        quote! {
            /// Liveness probe (Phase 5): 200 as long as the process is up and
            /// the async runtime is scheduling.  Never touches the database, so it
            /// never blocks on a write lock — the correct signal for a k8s
            /// `livenessProbe` / load-balancer health check.
            async fn __health() -> (StatusCode, Json<serde_json::Value>) {
                (StatusCode::OK, Json(json!({ "status": "ok" })))
            }

            /// Readiness probe (Phase 5): acquires a read lock on the database
            /// and returns 200 once obtained, proving the store opened and the
            /// lock is not wedged — the correct signal for a k8s `readinessProbe`.
            async fn __ready(
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let _guard = db.read().await;
                (StatusCode::OK, Json(json!({ "status": "ready" })))
            }

            #metrics_handler

            /// Snapshot token (#85): the current per-model row-count **watermark**
            /// of every model, captured atomically under one read guard on the
            /// single writer — a coherent "as of now" instant.  The client freezes
            /// this map and passes a model's watermark back as `?as_of=<w>` to that
            /// model's list/get for a point-in-time read.  Read-side peer of
            /// `/metrics`: opaque `usize` watermarks, a fixed per-schema key set,
            /// no field/relation/value decoded — so it is wired unauthenticated
            /// alongside the other ops routes (a process serves one tenant).  These
            /// watermarks are valid only within a compaction epoch: an in-process
            /// `compact()` renumbers physical rows, after which an older token is
            /// no longer comparable (the client must discard pinned tokens on a
            /// detected reopen).
            async fn __snapshot(
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let db = db.read().await;
                let watermarks = json!({ #(#metric_entries)* });
                (StatusCode::OK, Json(json!({ "watermarks": watermarks })))
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
        // #151 (Tier A): emit the `/metrics` route only when the handler is
        // emitted (`[server].metrics`); otherwise omit it entirely.
        let metrics_route = if Self::active_cfg().metrics {
            quote! { .route("/metrics", get(__metrics)) }
        } else {
            quote! {}
        };
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
            /// The data-plane routes (CRUD + WS subscriptions) with the database
            /// state still unbound.  Factored out so the tenant-auth guard can wrap
            /// ONLY these routes, leaving the operational endpoints unauthenticated
            /// (Phase 5).
            fn __data_routes() -> Router<Arc<RwLock<super::Database>>> {
                Router::new()
                    #(#routes)*
                    // Durable replication stream (#82 Direction C): one schema-wide
                    // endpoint behind the tenant-auth guard, so a follower receives
                    // only its own tenant's frames.
                    .route("/replicate", get(__replicate))
            }

            /// The operational routes (Phase 5): liveness / readiness / minimal
            /// metrics.  Never behind the tenant-auth guard so infra probes and
            /// metric scrapers reach them without a JWT.
            fn __ops_routes() -> Router<Arc<RwLock<super::Database>>> {
                Router::new()
                    .route("/health", get(__health))
                    .route("/ready", get(__ready))
                    #metrics_route
                    // Snapshot "as of now" token (#85): per-model watermarks.
                    .route("/snapshot", get(__snapshot))
            }

            /// Create the API router with all endpoints (no auth).  A
            /// `tower_http::trace::TraceLayer` wraps every route so each request is
            /// logged as a structured `tracing` span (level via `RUST_LOG`) — the
            /// server-side half of Phase 5 observability; the scaffold
            /// `main.rs` installs the subscriber.
            pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {
                __data_routes()
                    .merge(__ops_routes())
                    .layer(tower_http::trace::TraceLayer::new_for_http())
                    .with_state(db)
            }

            /// Create the API router with the tenant-auth guard layered over the
            /// data routes (#59).  Each data request must carry a bearer JWT whose
            /// configured tenant claim equals this process's tenant — the
            /// `forgedb-auth` substrate verifies the signature (asymmetric,
            /// JWKS/static key, algorithm-pinned) and cross-checks the tenant,
            /// rejecting with 401 (auth failure) or 403 (wrong tenant) before any
            /// handler runs; on success the verified `forgedb_auth::Principal` is
            /// injected into request extensions.  `auth` is built from deployment
            /// config (`forgedb.toml` / env), never from the `.forge` schema — the
            /// guard is a signed-string cross-check, not a schema-reading policy
            /// engine.
            ///
            /// The guard covers the WS `/subscribe`, `/live-query`, and
            /// `/replicate` routes (WS clients must send the token in the
            /// `Authorization` header — a documented limitation); it does NOT cover
            /// the operational
            /// `/health` / `/ready` / `/metrics` routes, which are merged in
            /// AFTER the guard so infra probes stay unauthenticated (Phase 5).
            pub fn create_router_with_auth(
                db: Arc<RwLock<super::Database>>,
                auth: Arc<forgedb_auth::Authenticator>,
            ) -> Router {
                let guarded = __data_routes().layer(axum::middleware::from_fn_with_state(
                    auth,
                    forgedb_auth::axum_mw::require_tenant,
                ));
                guarded
                    .merge(__ops_routes())
                    .layer(tower_http::trace::TraceLayer::new_for_http())
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
