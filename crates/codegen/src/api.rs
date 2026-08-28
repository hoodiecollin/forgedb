use crate::rust::RustGenerator;
use crate::{GenConfig, GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

thread_local! {
    static ACTIVE_CONFIG: std::cell::Cell<GenConfig> = const { std::cell::Cell::new(GenConfig::DEFAULT) };
}

pub struct ApiGenerator;

impl ApiGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Self::generate_with_config(schema, GenConfig::DEFAULT)
    }

    pub fn generate_with_config(schema: &Schema, config: GenConfig) -> Result<GeneratedCode> {
        ACTIVE_CONFIG.with(|c| c.set(config));
        let code = Self::generate_code(schema)?;

        Ok(GeneratedCode {
            code,
            description: format!("REST API server ({} models)", schema.models.len()),
        })
    }

    fn active_cfg() -> GenConfig {
        ACTIVE_CONFIG.with(|c| c.get())
    }

    fn generate_code(schema: &Schema) -> Result<String> {
        let mut tokens = TokenStream::new();

        let header = quote! {
        };
        tokens.extend(header);

        let inline_str_import = RustGenerator::inline_str_import(schema);
        let imports = quote! {
            #![allow(dead_code, unused_imports)]

            use super::*;

            use axum::{
                extract::{Path, Query, State},
                extract::ws::{Message, WebSocket, WebSocketUpgrade},
                http::StatusCode,
                response::{IntoResponse, Json, Response},
                routing::{delete, get, post, put},
                Router,
            };
            #inline_str_import
            use forgedb_types::{Timestamp, Uuid};
            use serde_json::json;
            use std::collections::HashMap;
            use std::sync::Arc;
            use tokio::sync::RwLock;
            use utoipa::OpenApi;
            use utoipa_axum::router::OpenApiRouter;
            use utoipa_axum::routes;
        };
        tokens.extend(imports);

        let cfg = Self::active_cfg();
        let __page_default_limit = proc_macro2::Literal::usize_unsuffixed(cfg.page_default_limit);
        let __page_max_limit = proc_macro2::Literal::usize_unsuffixed(cfg.page_max_limit);
        tokens.extend(quote! {
            const PAGE_DEFAULT_LIMIT: usize = #__page_default_limit;
            const PAGE_MAX_LIMIT: usize = #__page_max_limit;
        });

        tokens.extend(quote! {
            #[derive(serde::Serialize)]
            struct __ListEnvelope<'a, T: serde::Serialize> {
                data: &'a [T],
                total: usize,
                limit: usize,
                offset: usize,
            }
        });

        for model in &schema.models {
            let handler_tokens = Self::generate_handlers(schema, model)?;
            tokens.extend(handler_tokens);
        }

        for model in &schema.models {
            tokens.extend(Self::generate_subscription(model));
        }

        for model in &schema.models {
            tokens.extend(Self::generate_live_query(schema, model));
        }

        tokens.extend(Self::generate_replication_handler());

        tokens.extend(Self::generate_ops_handlers(schema));

        let openapi_tokens = Self::generate_openapi_doc(schema)?;
        tokens.extend(openapi_tokens);

        let router_tokens = Self::generate_router(schema)?;
        tokens.extend(router_tokens);

        let syntax_tree = syn::parse_file(&tokens.to_string())
            .map_err(|e| crate::CodegenError::GenerationFailed(format!("Failed to parse generated code: {}", e)))?;

        Ok(prettyplease::unparse(&syntax_tree))
    }

    fn id_parse_type(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        RustGenerator::id_type_tokens(schema, model)
    }

    fn id_field_ident(model: &forgedb_parser::Model) -> proc_macro2::Ident {
        match model.identity_field() {
            Some(f) => format_ident!("{}", f.name),
            None => format_ident!("id"),
        }
    }

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
                    Some(r) => (StatusCode::OK, Json(r)).into_response(),
                    None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))
                        .into_response(),
                },
            });
            list_arms.push(quote! {
                #name => {
                    let __data: Vec<super::#proj_ident> = page
                        .iter()
                        .map(|r| super::#proj_ident { #(#field_copies),* })
                        .collect();
                    (
                        StatusCode::OK,
                        Json(__ListEnvelope {
                            data: &__data,
                            total,
                            limit: qp.pagination.limit,
                            offset: qp.pagination.offset,
                        }),
                    )
                        .into_response()
                }
            });
        }

        let get_block = quote! {
            if let Some(__proj) = params.get("projection") {
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" })))
                        .into_response(),
                };
                let db = db.read().await;
                return match __proj.as_str() {
                    #(#get_arms)*
                    _ => (StatusCode::BAD_REQUEST, Json(json!({ "error": "unknown projection" })))
                        .into_response(),
                };
            }
        };
        let list_block = quote! {
            if let Some(__proj) = params.get("projection") {
                return match __proj.as_str() {
                    #(#list_arms)*
                    _ => (StatusCode::BAD_REQUEST, Json(json!({ "error": "unknown projection" })))
                        .into_response(),
                };
            }
        };
        (get_block, list_block)
    }

    fn generate_handlers(schema: &Schema, model: &forgedb_parser::Model) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let id_type = Self::id_parse_type(schema, model);
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
        let has_id = model.identity_field().is_some();
        let id_field = Self::id_field_ident(model);
        let scan_matches_fn = format_ident!("__{}_scan_matches", Self::to_snake_case(&model.name));
        let is_unfiltered_fn =
            format_ident!("__{}_is_unfiltered", Self::to_snake_case(&model.name));
        let scan_sort_fn = format_ident!("__{}_scan_sort", Self::to_snake_case(&model.name));
        let scan_ref_ident = format_ident!("{}ScanRef", model.name);
        let pushdown_fields = crate::rust::RustGenerator::scan_pushdown_fields(model);
        let row_selection = if pushdown_fields.is_empty() {
            quote! { None }
        } else {
            let branches = pushdown_fields.iter().map(|f| {
                let fname = &f.name;
                let rows_by = format_ident!("__rows_by_{}", f.name);
                quote! {
                    if let Some(__v) = params.get(#fname) {
                        db.#storage_field.#rows_by(__v)
                    }
                }
            });
            quote! { #(#branches else)* { None } }
        };
        let page_ref_ident = format_ident!("{}PageRef", model.name);
        let page_envelope_closure = quote! {
            |__total: usize, __page: &[super::#page_ref_ident<'_>]| {
                (
                    StatusCode::OK,
                    Json(__ListEnvelope {
                        data: __page,
                        total: __total,
                        limit: qp.pagination.limit,
                        offset: qp.pagination.offset,
                    }),
                )
                    .into_response()
            }
        };
        let page_scope_return = quote! {
            let __keep_all: bool = #is_unfiltered_fn(&params);
            if __keep_all && qp.sort.is_none() {
                return db.#storage_field.__with_fast_page(
                    qp.pagination.offset,
                    qp.pagination.limit,
                    #page_envelope_closure,
                );
            }
            let __sel: Option<Vec<usize>> = #row_selection;
            return db.#storage_field.__with_page(
                __sel,
                |r| __keep_all || #scan_matches_fn(r, &params),
                |__scan: &mut Vec<super::#scan_ref_ident<'_>>| {
                    #scan_sort_fn(__scan, &qp.sort);
                },
                qp.pagination.offset,
                qp.pagination.limit,
                #page_envelope_closure,
            );
        };
        let owned_narrow_block = quote! {
            let __sel: Option<Vec<usize>> = #row_selection;
            let __keep_all: bool = #is_unfiltered_fn(&params);
            let (total, __page_ids) = db.#storage_field.__with_scan(
                __sel,
                |r| __keep_all || #scan_matches_fn(r, &params),
                |__scan: &mut Vec<super::#scan_ref_ident<'_>>| {
                    #scan_sort_fn(__scan, &qp.sort);
                    let __total = __scan.len();
                    let __ids: Vec<_> = qp.pagination
                        .apply(__scan)
                        .iter()
                        .map(|r| r.#id_field)
                        .collect();
                    (__total, __ids)
                },
            );
            let page: Vec<super::#model_name> = __page_ids
                .iter()
                .filter_map(|__id| db.#storage_field.get(*__id))
                .collect();
        };
        let live_list_arm = if !has_id {
            quote! {
                {
                    let mut rows: Vec<super::#model_name> = db.#storage_field.all()
                        .into_iter()
                        .filter(|r| #filter_fn(r, &params))
                        .collect();
                    #sort_fn(&mut rows, &qp.sort);
                    let total = rows.len();
                    let page: Vec<super::#model_name> = qp.pagination.apply(&rows).to_vec();
                    (page, total)
                }
            }
        } else if model.projections.is_empty() {
            quote! { { #page_scope_return } }
        } else {
            quote! {
                {
                    if params.get("projection").is_none() {
                        #page_scope_return
                    }
                    #owned_narrow_block
                    (page, total)
                }
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
            async fn #list_fn(
                Query(params): Query<HashMap<String, String>>,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                let mut qp = forgedb_query_params::QueryParams::from_map(params.clone());
                qp.pagination.limit = params
                    .get("limit")
                    .and_then(|__s| __s.parse::<usize>().ok())
                    .unwrap_or(PAGE_DEFAULT_LIMIT)
                    .clamp(1, PAGE_MAX_LIMIT);
                let __as_of: Option<usize> = match params.get("as_of") {
                    Some(__w) => match __w.parse::<usize>() {
                        Ok(__n) => Some(__n),
                        Err(_) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({ "error": "as_of must be a non-negative integer watermark" })),
                            )
                                .into_response();
                        }
                    },
                    None => None,
                };
                let db = db.read().await;
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
                    None => #live_list_arm,
                };
                #proj_list_block
                (
                    StatusCode::OK,
                    Json(__ListEnvelope {
                        data: &page,
                        total,
                        limit: qp.pagination.limit,
                        offset: qp.pagination.offset,
                    }),
                )
                    .into_response()
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
            ) -> Response {
                #proj_get_block
                let key = match id.parse::<#id_type>() {
                    Ok(key) => key,
                    Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid id" })))
                        .into_response(),
                };
                let __as_of: Option<usize> = match params.get("as_of") {
                    Some(__w) => match __w.parse::<usize>() {
                        Ok(__n) => Some(__n),
                        Err(_) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({ "error": "as_of must be a non-negative integer watermark" })),
                            )
                                .into_response();
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
                    Some(record) => (StatusCode::OK, Json(record)).into_response(),
                    None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))
                        .into_response(),
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

    fn list_sort_arms(model: &forgedb_parser::Model) -> Vec<TokenStream> {
        model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| {
                let fname = &f.name;
                let fident = format_ident!("{}", f.name);
                if Self::is_float_field(&f.field_type) {
                    quote! {
                        #fname => rows.sort_by(|a, b| {
                            a.#fident.partial_cmp(&b.#fident)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        }),
                    }
                } else {
                    quote! {
                        #fname => rows.sort_by(|a, b| a.#fident.cmp(&b.#fident)),
                    }
                }
            })
            .collect()
    }

    fn generate_list_scan_helpers(model: &forgedb_parser::Model) -> TokenStream {
        if model.identity_field().is_none() {
            return quote! {};
        }
        let scan_ref_ident = format_ident!("{}ScanRef", model.name);
        let snake = Self::to_snake_case(&model.name);
        let scan_matches_fn = format_ident!("__{}_scan_matches", snake);
        let scan_sort_fn = format_ident!("__{}_scan_sort", snake);
        let filterable: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .collect();
        let field_checks: Vec<_> = filterable
            .iter()
            .map(|f| Self::generate_filter_check(f, true))
            .collect();
        let is_unfiltered_fn = format_ident!("__{}_is_unfiltered", snake);
        let unfiltered_checks: Vec<_> = filterable
            .iter()
            .map(|f| {
                let fname_str = &f.name;
                quote! { if params.contains_key(#fname_str) { return false; } }
            })
            .collect();
        let unfiltered_param = if unfiltered_checks.is_empty() {
            format_ident!("_params")
        } else {
            format_ident!("params")
        };
        let arms = Self::list_sort_arms(model);
        quote! {
            fn #is_unfiltered_fn(#unfiltered_param: &HashMap<String, String>) -> bool {
                #(#unfiltered_checks)*
                true
            }

            fn #scan_matches_fn(
                record: &super::#scan_ref_ident<'_>,
                params: &HashMap<String, String>,
            ) -> bool {
                if params.is_empty() {
                    return true;
                }
                #(#field_checks)*
                true
            }

            fn #scan_sort_fn(
                rows: &mut Vec<super::#scan_ref_ident<'_>>,
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

    fn is_float_field(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::F64 => true,
            FieldType::Nullable(inner) => Self::is_float_field(inner),
            _ => false,
        }
    }

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
            | FieldType::Timestamp(_)
            | FieldType::String
            | FieldType::StringN { .. }
            | FieldType::Decimal
            | FieldType::Enum(_)
            | FieldType::Bytes(_) => true,
            FieldType::Nullable(inner) => Self::is_filterable_field(inner),
            _ => false,
        }
    }

    fn peel_nullable(
        field_type: &forgedb_parser::FieldType,
    ) -> (&forgedb_parser::FieldType, bool) {
        match field_type {
            forgedb_parser::FieldType::Nullable(inner) => (inner.as_ref(), true),
            other => (other, false),
        }
    }

    fn generate_filter_check(field: &forgedb_parser::Field, borrowed: bool) -> TokenStream {
        use forgedb_parser::FieldType;
        let fname_str = &field.name;
        let fname = format_ident!("{}", field.name);
        let (base, nullable) = Self::peel_nullable(&field.field_type);

        if let FieldType::Bytes(n) = base {
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

        let parse: TokenStream = match base {
            FieldType::U32 => quote! { want.parse::<u32>().ok() },
            FieldType::U64 => quote! { want.parse::<u64>().ok() },
            FieldType::I32 => quote! { want.parse::<i32>().ok() },
            FieldType::I64 => quote! { want.parse::<i64>().ok() },
            FieldType::F64 => quote! { want.parse::<f64>().ok() },
            FieldType::Bool => quote! { want.parse::<bool>().ok() },
            FieldType::String | FieldType::StringN { .. } => quote! { Some(want.clone()) },
            FieldType::Uuid => quote! { want.parse::<Uuid>().ok() },
            FieldType::Decimal => quote! { want.parse::<rust_decimal::Decimal>().ok() },
            FieldType::Timestamp(p) => {
                let quantum = p.quantum_micros();
                if quantum > 1 {
                    quote! {
                        want.parse::<forgedb_types::Timestamp>()
                            .ok()
                            .map(|__ts| __ts.floor_to_micros(#quantum))
                    }
                } else {
                    quote! { want.parse::<forgedb_types::Timestamp>().ok() }
                }
            }
            FieldType::Enum(name) => {
                let en = format_ident!("{}", name);
                quote! {
                    serde_json::from_value::<super::#en>(
                        serde_json::Value::String(want.clone())
                    ).ok()
                }
            }
            _ => return quote! {},
        };

        let cmp = if nullable {
            if borrowed && matches!(base, FieldType::String | FieldType::StringN { .. }) {
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
            fn #changed_fn(a: &super::#model_name, b: &super::#model_name) -> bool {
                #(#field_cmps)*
                false
            }
        }
    }

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
        let model_name_str = &model.name;

        let field_checks: Vec<_> = model
            .fields
            .iter()
            .filter(|f| Self::is_filterable_field(&f.field_type))
            .map(|f| Self::generate_filter_check(f, false))
            .collect();

        let record_changed = Self::generate_record_changed(model);


        quote! {
            fn #filter_fn(record: &super::#model_name, params: &HashMap<String, String>) -> bool {
                if params.is_empty() {
                    return true;
                }
                #(#field_checks)*
                true
            }

            #record_changed

            async fn #subscribe_fn(
                Query(params): Query<HashMap<String, String>>,
                headers: axum::http::HeaderMap,
                axum::Extension(allowed): axum::Extension<AllowedOrigins>,
                ws: WebSocketUpgrade,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                if !allowed.permits(__origin_of(&headers)) {
                    return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
                }
                ws.on_upgrade(move |socket| #handle_fn(socket, db, params))
            }

            async fn #handle_fn(
                mut socket: WebSocket,
                db: Arc<RwLock<super::Database>>,
                params: HashMap<String, String>,
            ) {
                let mut rx = { db.read().await.changefeed.subscribe() };
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if event.model != #model_name_str {
                                continue;
                            }
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
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    fn generate_replication_handler() -> TokenStream {
        quote! {
            async fn __replicate(
                Query(params): Query<HashMap<String, String>>,
                headers: axum::http::HeaderMap,
                axum::Extension(allowed): axum::Extension<AllowedOrigins>,
                ws: WebSocketUpgrade,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                if !allowed.permits(__origin_of(&headers)) {
                    return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
                }
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

                let broker = { db.read().await.broker.clone() };
                let Some(broker) = broker else {
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                };

                let catch = match broker.lock() {
                    Ok(b) => b.catch_up_from(after, usize::MAX),
                    Err(_) => return,
                };
                let Ok(mut catch) = catch else { return };

                for ev in &catch.replayed {
                    if socket
                        .send(Message::Binary(ev.to_wire().into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }

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
                                break;
                            }
                        }
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

    fn generate_live_query(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let delta_name = format_ident!("{}LiveDelta", model.name);
        let snake = Self::to_snake_case(&model.name);
        let storage_field = format_ident!("{}", snake);
        let subscribe_fn = format_ident!("subscribe_live_{}", snake);
        let handle_fn = format_ident!("handle_{}_live_query", snake);
        let changed_fn = format_ident!("{}_record_changed", snake);
        let scan_matches_fn = format_ident!("__{}_scan_matches", snake);
        let is_unfiltered_fn = format_ident!("__{}_is_unfiltered", snake);
        let scan_ref_ident = format_ident!("{}ScanRef", model.name);
        let id_field = Self::id_field_ident(model);
        let id_type = Self::id_parse_type(schema, model);
        let model_name_str = &model.name;


        quote! {
            async fn #subscribe_fn(
                Query(params): Query<HashMap<String, String>>,
                headers: axum::http::HeaderMap,
                axum::Extension(allowed): axum::Extension<AllowedOrigins>,
                ws: WebSocketUpgrade,
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> Response {
                if !allowed.permits(__origin_of(&headers)) {
                    return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
                }
                ws.on_upgrade(move |socket| #handle_fn(socket, db, params))
            }

            async fn #handle_fn(
                mut socket: WebSocket,
                db: Arc<RwLock<super::Database>>,
                params: HashMap<String, String>,
            ) {
                let __keep_all: bool = #is_unfiltered_fn(&params);

                let mut rx = { db.read().await.changefeed.subscribe() };

                let mut members: HashMap<#id_type, super::#model_name> = HashMap::new();

                {
                    let rows: Vec<super::#model_name> = {
                        let g = db.read().await;
                        let __ids = g.#storage_field.__with_scan(
                            None,
                            |r| __keep_all || #scan_matches_fn(r, &params),
                            |__scan: &mut Vec<super::#scan_ref_ident<'_>>| {
                                __scan.iter().map(|r| r.#id_field).collect::<Vec<_>>()
                            },
                        );
                        __ids.into_iter()
                            .filter_map(|__id| g.#storage_field.get(__id))
                            .collect()
                    };
                    for r in &rows {
                        members.insert(r.#id_field, r.clone());
                    }
                    let init = super::#delta_name::Init { rows };
                    if let Ok(text) = serde_json::to_string(&init) {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                }

                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if event.model != #model_name_str {
                                continue;
                            }

                            let current: Vec<super::#model_name> = {
                                let g = db.read().await;
                                let __ids = g.#storage_field.__with_scan(
                                    None,
                                    |r| __keep_all || #scan_matches_fn(r, &params),
                                    |__scan: &mut Vec<super::#scan_ref_ident<'_>>| {
                                        __scan.iter().map(|r| r.#id_field).collect::<Vec<_>>()
                                    },
                                );
                                __ids.into_iter()
                                    .filter_map(|__id| g.#storage_field.get(__id))
                                    .collect()
                            };

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
                                    return;
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

    fn generate_ops_handlers(schema: &Schema) -> TokenStream {
        let metric_entries: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let name = &model.name;
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { #name: db.#field.row_count(), }
            })
            .collect();

        let metrics_handler = if Self::active_cfg().metrics {
            let sum_fields: Vec<_> = schema
                .models
                .iter()
                .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
                .collect();
            let model_count = schema.models.len();
            quote! {
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
            async fn __health() -> (StatusCode, Json<serde_json::Value>) {
                (StatusCode::OK, Json(json!({ "status": "ok" })))
            }

            async fn __ready(
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let _guard = db.read().await;
                (StatusCode::OK, Json(json!({ "status": "ready" })))
            }

            #metrics_handler

            async fn __snapshot(
                State(db): State<Arc<RwLock<super::Database>>>,
            ) -> (StatusCode, Json<serde_json::Value>) {
                let db = db.read().await;
                let watermarks = json!({ #(#metric_entries)* });
                (StatusCode::OK, Json(json!({ "watermarks": watermarks })))
            }
        }
    }

    fn generate_openapi_doc(schema: &Schema) -> Result<TokenStream> {
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

            pub fn openapi_json() -> String {
                ApiDoc::openapi().to_json().unwrap()
            }
        };

        Ok(tokens)
    }

    fn generate_router(schema: &Schema) -> Result<TokenStream> {
        let metrics_route = if Self::active_cfg().metrics {
            quote! { .route("/metrics", get(__metrics)) }
        } else {
            quote! {}
        };
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
                    .route(
                        concat!("/api/", #route_path, "/{id}"),
                        get(#get_fn).put(#update_fn).delete(#delete_fn),
                    )
                    .route(concat!("/subscribe/", #route_path), get(#subscribe_fn))
                    .route(concat!("/live-query/", #route_path), get(#live_query_fn))
                }
            })
            .collect();

        let tokens = quote! {
            fn __data_routes() -> Router<Arc<RwLock<super::Database>>> {
                Router::new()
                    #(#routes)*
                    .route("/replicate", get(__replicate))
            }

            fn __ops_routes() -> Router<Arc<RwLock<super::Database>>> {
                Router::new()
                    .route("/health", get(__health))
                    .route("/ready", get(__ready))
                    #metrics_route
                    .route("/snapshot", get(__snapshot))
            }

            pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {
                create_router_with_options(db, HttpOptions::default())
            }

            #[derive(Debug, Clone, Default)]
            pub struct HttpOptions {
                pub allowed_origins: Option<Vec<String>>,
            }

            pub fn parse_origins(raw: &str) -> Result<Option<Vec<String>>, String> {
                let parts: Vec<String> = raw
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                if parts.is_empty() {
                    return Ok(None);
                }
                let wildcards = parts.iter().filter(|p| p.as_str() == "*").count();
                if wildcards > 0 && parts.len() > 1 {
                    return Err(
                        "`*` cannot be combined with explicit origins — use either a \
                         single `*` or an explicit list"
                            .to_string(),
                    );
                }
                for p in &parts {
                    if axum::http::HeaderValue::from_str(p).is_err() {
                        return Err(format!("`{p}` is not a valid origin header value"));
                    }
                }
                Ok(Some(parts))
            }

            #[derive(Debug, Clone)]
            pub struct AllowedOrigins(pub Option<Arc<Vec<String>>>);

            impl AllowedOrigins {
                pub fn permits(&self, origin: Option<&str>) -> bool {
                    match (&self.0, origin) {
                        (None, _) => true,
                        (Some(_), None) => true,
                        (Some(list), Some(o)) => {
                            list.iter().any(|a| a == "*" || a == o)
                        }
                    }
                }
            }

            fn __origin_of(headers: &axum::http::HeaderMap) -> Option<&str> {
                headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok())
            }

            fn __cors_layer(origins: &Option<Arc<Vec<String>>>) -> Option<tower_http::cors::CorsLayer> {
                let list = origins.as_ref()?;
                let methods = [
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                ];
                let headers = [
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                ];
                let layer = tower_http::cors::CorsLayer::new()
                    .allow_methods(methods)
                    .allow_headers(headers);
                if list.iter().any(|o| o == "*") {
                    return Some(layer.allow_origin(tower_http::cors::Any));
                }
                let parsed: Vec<axum::http::HeaderValue> = list
                    .iter()
                    .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
                    .collect();
                Some(layer.allow_origin(parsed))
            }

            fn __apply_origin_layers(router: Router<Arc<RwLock<super::Database>>>, opts: HttpOptions)
                -> Router<Arc<RwLock<super::Database>>>
            {
                let origins = opts.allowed_origins.map(Arc::new);
                let cors = __cors_layer(&origins);
                let router = router.layer(axum::Extension(AllowedOrigins(origins)));
                match cors {
                    Some(layer) => router.layer(layer),
                    None => router,
                }
            }

            pub fn create_router_with_options(
                db: Arc<RwLock<super::Database>>,
                opts: HttpOptions,
            ) -> Router {
                let router = __data_routes()
                    .merge(__ops_routes())
                    .layer(tower_http::trace::TraceLayer::new_for_http());
                __apply_origin_layers(router, opts).with_state(db)
            }

            pub fn create_router_with_auth(
                db: Arc<RwLock<super::Database>>,
                auth: Arc<forgedb_auth::Authenticator>,
            ) -> Router {
                create_router_with_auth_and_options(db, auth, HttpOptions::default())
            }

            pub fn create_router_with_auth_and_options(
                db: Arc<RwLock<super::Database>>,
                auth: Arc<forgedb_auth::Authenticator>,
                opts: HttpOptions,
            ) -> Router {
                let guarded = __data_routes().layer(axum::middleware::from_fn_with_state(
                    auth,
                    forgedb_auth::axum_mw::require_tenant,
                ));
                let router = guarded
                    .merge(__ops_routes())
                    .layer(tower_http::trace::TraceLayer::new_for_http());
                __apply_origin_layers(router, opts).with_state(db)
            }
        };

        Ok(tokens)
    }

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
