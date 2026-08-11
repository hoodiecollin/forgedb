//! Generated API server by ForgeDB
//! DO NOT EDIT - This file is auto-generated
#![allow(dead_code, unused_imports)]
use super::*;
use axum::{
    extract::{Path, Query, State},
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode, response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use forgedb_types::{Timestamp, Uuid};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
const PAGE_DEFAULT_LIMIT: usize = 50;
const PAGE_MAX_LIMIT: usize = 1000;
#[derive(serde::Serialize)]
struct __ListEnvelope<'a, T: serde::Serialize> {
    data: &'a [T],
    total: usize,
    limit: usize,
    offset: usize,
}
#[utoipa::path(
    get,
    path = "",
    tag = "User",
    params(
        (
            "limit" = Option<usize>,
            Query,
            description = "Max rows (clamped to [1, 1000]; default 50)"
        ),
        ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
        ("sort" = Option<String>, Query, description = "Field to sort by"),
        ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        ),
    ),
    responses((status = 200, description = "List all User", body = Vec<User>))
)]
async fn list_user(
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
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let (page, total) = match __as_of {
        Some(__w) => {
            let mut rows: Vec<super::User> = db
                .user
                .all_at(&forgedb_storage::Snapshot::new(__w))
                .into_iter()
                .filter(|r| user_event_matches(r, &params))
                .collect();
            user_apply_sort(&mut rows, &qp.sort);
            let total = rows.len();
            let page: Vec<super::User> = qp.pagination.apply(&rows).to_vec();
            (page, total)
        }
        None => {
            let __keep_all: bool = __user_is_unfiltered(&params);
            if __keep_all && qp.sort.is_none() {
                return db
                    .user
                    .__with_fast_page(
                        qp.pagination.offset,
                        qp.pagination.limit,
                        |__total: usize, __page: &[super::UserPageRef<'_>]| {
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
                        },
                    );
            }
            let __sel: Option<Vec<usize>> = if let Some(__v) = params.get("email") {
                db.user.__rows_by_email(__v)
            } else {
                None
            };
            return db
                .user
                .__with_page(
                    __sel,
                    |r| __keep_all || __user_scan_matches(r, &params),
                    |__scan: &mut Vec<super::UserScanRef<'_>>| {
                        __user_scan_sort(__scan, &qp.sort);
                    },
                    qp.pagination.offset,
                    qp.pagination.limit,
                    |__total: usize, __page: &[super::UserPageRef<'_>]| {
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
                    },
                );
        }
    };
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
    tag = "User",
    params(
        ("id" = String, Path, description = "User"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        )
    ),
    responses(
        (status = 200, description = "Get User by ID", body = User),
        (status = 404, description = "Not found")
    )
)]
async fn get_user(
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                .into_response();
        }
    };
    let __as_of: Option<usize> = match params.get("as_of") {
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let __found = match __as_of {
        Some(__w) => db.user.get_at(&forgedb_storage::Snapshot::new(__w), key),
        None => db.user.get(key),
    };
    match __found {
        Some(record) => (StatusCode::OK, Json(record)).into_response(),
        None => {
            (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                .into_response()
        }
    }
}
#[utoipa::path(
    post,
    path = "",
    tag = "User",
    request_body = User,
    responses(
        (status = 201, description = "Create new User", body = User),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn create_user(
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match serde_json::from_value::<super::User>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.create_user(record) {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id" : id.to_string() }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "User",
    params(("id" = String, Path, description = "User")),
    request_body = User,
    responses(
        (status = 200, description = "Replace User by ID", body = User),
        (status = 404, description = "Not found"),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn update_user(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let record = match serde_json::from_value::<super::User>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.update_user(key, record) {
        Ok(true) => (StatusCode::OK, Json(json!({ "id" : key.to_string() }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "User",
    params(("id" = String, Path, description = "User")),
    responses(
        (status = 204, description = "Delete User by ID"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Referenced by children (on_delete=restrict)")
    )
)]
async fn delete_user(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let mut db = db.write().await;
    match db.delete_user(key) {
        Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::CONFLICT);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
fn user_apply_sort(
    rows: &mut Vec<super::User>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "name" => rows.sort_by(|a, b| a.name.cmp(&b.name)),
        "email" => rows.sort_by(|a, b| a.email.cmp(&b.email)),
        "created_at" => rows.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
/// Is this request unfiltered — does NO query key name a filterable field
/// of this model (#288)?
///
/// Hoisted out of the per-row loop by every caller, because it is a
/// property of the query string and not of the row. `_scan_matches` can
/// only short-circuit on `params.is_empty()`, and `?limit=50` defeats that
/// while naming no filterable field at all — so an unfiltered request was
/// running one `HashMap` lookup per filterable field per SCANNED ROW, all
/// of them guaranteed to miss. Measured at ~50 ns/row, i.e. 502 µs of a
/// 850 µs request over 10k rows.
///
/// The question is POSITIVE — "does any key name a field of this model?" —
/// deliberately. A negative exclusion list (`limit`/`offset`/`sort`/
/// `order`/`as_of`) would need maintaining, and would be wrong: a model may
/// legally declare a field named `limit`, and for that model `?limit=3`
/// genuinely is a filter.
fn __user_is_unfiltered(params: &HashMap<String, String>) -> bool {
    if params.contains_key("id") {
        return false;
    }
    if params.contains_key("name") {
        return false;
    }
    if params.contains_key("email") {
        return false;
    }
    if params.contains_key("created_at") {
        return false;
    }
    true
}
/// Narrow closed-set filter over the BORROWED scan view (#160/#224), so
/// a row is accepted or rejected before its strings are ever copied out
/// of the buffered column.  Same per-field checks as `_event_matches`,
/// only the operand type is narrower.
fn __user_scan_matches(
    record: &super::UserScanRef<'_>,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("name") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.name == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("email") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.email == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("created_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.created_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Narrow list sort over the borrowed scan view (#160/#228) — same arms
/// as `_apply_sort`.  Runs inside the scan scope, on views that borrow
/// the buffered columns: `&str` and `Option<&str>` are `Ord`, so
/// comparing them is comparing the buffer's bytes in place.
fn __user_scan_sort(
    rows: &mut Vec<super::UserScanRef<'_>>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "name" => rows.sort_by(|a, b| a.name.cmp(&b.name)),
        "email" => rows.sort_by(|a, b| a.email.cmp(&b.email)),
        "created_at" => rows.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
#[utoipa::path(
    get,
    path = "",
    tag = "Post",
    params(
        (
            "limit" = Option<usize>,
            Query,
            description = "Max rows (clamped to [1, 1000]; default 50)"
        ),
        ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
        ("sort" = Option<String>, Query, description = "Field to sort by"),
        ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        ),
    ),
    responses((status = 200, description = "List all Post", body = Vec<Post>))
)]
async fn list_post(
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
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let (page, total) = match __as_of {
        Some(__w) => {
            let mut rows: Vec<super::Post> = db
                .post
                .all_at(&forgedb_storage::Snapshot::new(__w))
                .into_iter()
                .filter(|r| post_event_matches(r, &params))
                .collect();
            post_apply_sort(&mut rows, &qp.sort);
            let total = rows.len();
            let page: Vec<super::Post> = qp.pagination.apply(&rows).to_vec();
            (page, total)
        }
        None => {
            if params.get("projection").is_none() {
                let __keep_all: bool = __post_is_unfiltered(&params);
                if __keep_all && qp.sort.is_none() {
                    return db
                        .post
                        .__with_fast_page(
                            qp.pagination.offset,
                            qp.pagination.limit,
                            |__total: usize, __page: &[super::PostPageRef<'_>]| {
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
                            },
                        );
                }
                let __sel: Option<Vec<usize>> = if let Some(__v) = params.get("views") {
                    db.post.__rows_by_views(__v)
                } else {
                    None
                };
                return db
                    .post
                    .__with_page(
                        __sel,
                        |r| __keep_all || __post_scan_matches(r, &params),
                        |__scan: &mut Vec<super::PostScanRef<'_>>| {
                            __post_scan_sort(__scan, &qp.sort);
                        },
                        qp.pagination.offset,
                        qp.pagination.limit,
                        |__total: usize, __page: &[super::PostPageRef<'_>]| {
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
                        },
                    );
            }
            let __sel: Option<Vec<usize>> = if let Some(__v) = params.get("views") {
                db.post.__rows_by_views(__v)
            } else {
                None
            };
            let __keep_all: bool = __post_is_unfiltered(&params);
            let (total, __page_ids) = db
                .post
                .__with_scan(
                    __sel,
                    |r| __keep_all || __post_scan_matches(r, &params),
                    |__scan: &mut Vec<super::PostScanRef<'_>>| {
                        __post_scan_sort(__scan, &qp.sort);
                        let __total = __scan.len();
                        let __ids: Vec<_> = qp
                            .pagination
                            .apply(__scan)
                            .iter()
                            .map(|r| r.id)
                            .collect();
                        (__total, __ids)
                    },
                );
            let page: Vec<super::Post> = __page_ids
                .iter()
                .filter_map(|__id| db.post.get(*__id))
                .collect();
            (page, total)
        }
    };
    if let Some(__proj) = params.get("projection") {
        return match __proj.as_str() {
            "agg" => {
                let __data: Vec<super::PostAgg> = page
                    .iter()
                    .map(|r| super::PostAgg {
                        id: r.id.clone(),
                        views: r.views.clone(),
                        published: r.published.clone(),
                    })
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
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
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
    tag = "Post",
    params(
        ("id" = String, Path, description = "Post"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        )
    ),
    responses(
        (status = 200, description = "Get Post by ID", body = Post),
        (status = 404, description = "Not found")
    )
)]
async fn get_post(
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if let Some(__proj) = params.get("projection") {
        let key = match id.parse::<Uuid>() {
            Ok(key) => key,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                    .into_response();
            }
        };
        let db = db.read().await;
        return match __proj.as_str() {
            "agg" => {
                match db.post.get_agg(key) {
                    Some(r) => (StatusCode::OK, Json(r)).into_response(),
                    None => {
                        (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                            .into_response()
                    }
                }
            }
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                .into_response();
        }
    };
    let __as_of: Option<usize> = match params.get("as_of") {
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let __found = match __as_of {
        Some(__w) => db.post.get_at(&forgedb_storage::Snapshot::new(__w), key),
        None => db.post.get(key),
    };
    match __found {
        Some(record) => (StatusCode::OK, Json(record)).into_response(),
        None => {
            (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                .into_response()
        }
    }
}
#[utoipa::path(
    post,
    path = "",
    tag = "Post",
    request_body = Post,
    responses(
        (status = 201, description = "Create new Post", body = Post),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn create_post(
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match serde_json::from_value::<super::Post>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.create_post(record) {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id" : id.to_string() }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Post",
    params(("id" = String, Path, description = "Post")),
    request_body = Post,
    responses(
        (status = 200, description = "Replace Post by ID", body = Post),
        (status = 404, description = "Not found"),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn update_post(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let record = match serde_json::from_value::<super::Post>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.update_post(key, record) {
        Ok(true) => (StatusCode::OK, Json(json!({ "id" : key.to_string() }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Post",
    params(("id" = String, Path, description = "Post")),
    responses(
        (status = 204, description = "Delete Post by ID"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Referenced by children (on_delete=restrict)")
    )
)]
async fn delete_post(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let mut db = db.write().await;
    match db.delete_post(key) {
        Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::CONFLICT);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
fn post_apply_sort(
    rows: &mut Vec<super::Post>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "title" => rows.sort_by(|a, b| a.title.cmp(&b.title)),
        "views" => rows.sort_by(|a, b| a.views.cmp(&b.views)),
        "published" => rows.sort_by(|a, b| a.published.cmp(&b.published)),
        "created_at" => rows.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
/// Is this request unfiltered — does NO query key name a filterable field
/// of this model (#288)?
///
/// Hoisted out of the per-row loop by every caller, because it is a
/// property of the query string and not of the row. `_scan_matches` can
/// only short-circuit on `params.is_empty()`, and `?limit=50` defeats that
/// while naming no filterable field at all — so an unfiltered request was
/// running one `HashMap` lookup per filterable field per SCANNED ROW, all
/// of them guaranteed to miss. Measured at ~50 ns/row, i.e. 502 µs of a
/// 850 µs request over 10k rows.
///
/// The question is POSITIVE — "does any key name a field of this model?" —
/// deliberately. A negative exclusion list (`limit`/`offset`/`sort`/
/// `order`/`as_of`) would need maintaining, and would be wrong: a model may
/// legally declare a field named `limit`, and for that model `?limit=3`
/// genuinely is a filter.
fn __post_is_unfiltered(params: &HashMap<String, String>) -> bool {
    if params.contains_key("id") {
        return false;
    }
    if params.contains_key("title") {
        return false;
    }
    if params.contains_key("views") {
        return false;
    }
    if params.contains_key("published") {
        return false;
    }
    if params.contains_key("created_at") {
        return false;
    }
    true
}
/// Narrow closed-set filter over the BORROWED scan view (#160/#224), so
/// a row is accepted or rejected before its strings are ever copied out
/// of the buffered column.  Same per-field checks as `_event_matches`,
/// only the operand type is narrower.
fn __post_scan_matches(
    record: &super::PostScanRef<'_>,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("title") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.title == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("views") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.views == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("published") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.published == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("created_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.created_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Narrow list sort over the borrowed scan view (#160/#228) — same arms
/// as `_apply_sort`.  Runs inside the scan scope, on views that borrow
/// the buffered columns: `&str` and `Option<&str>` are `Ord`, so
/// comparing them is comparing the buffer's bytes in place.
fn __post_scan_sort(
    rows: &mut Vec<super::PostScanRef<'_>>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "title" => rows.sort_by(|a, b| a.title.cmp(&b.title)),
        "views" => rows.sort_by(|a, b| a.views.cmp(&b.views)),
        "published" => rows.sort_by(|a, b| a.published.cmp(&b.published)),
        "created_at" => rows.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
#[utoipa::path(
    get,
    path = "",
    tag = "Tag",
    params(
        (
            "limit" = Option<usize>,
            Query,
            description = "Max rows (clamped to [1, 1000]; default 50)"
        ),
        ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
        ("sort" = Option<String>, Query, description = "Field to sort by"),
        ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        ),
    ),
    responses((status = 200, description = "List all Tag", body = Vec<Tag>))
)]
async fn list_tag(
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
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let (page, total) = match __as_of {
        Some(__w) => {
            let mut rows: Vec<super::Tag> = db
                .tag
                .all_at(&forgedb_storage::Snapshot::new(__w))
                .into_iter()
                .filter(|r| tag_event_matches(r, &params))
                .collect();
            tag_apply_sort(&mut rows, &qp.sort);
            let total = rows.len();
            let page: Vec<super::Tag> = qp.pagination.apply(&rows).to_vec();
            (page, total)
        }
        None => {
            let __keep_all: bool = __tag_is_unfiltered(&params);
            if __keep_all && qp.sort.is_none() {
                return db
                    .tag
                    .__with_fast_page(
                        qp.pagination.offset,
                        qp.pagination.limit,
                        |__total: usize, __page: &[super::TagPageRef<'_>]| {
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
                        },
                    );
            }
            let __sel: Option<Vec<usize>> = if let Some(__v) = params.get("name") {
                db.tag.__rows_by_name(__v)
            } else {
                None
            };
            return db
                .tag
                .__with_page(
                    __sel,
                    |r| __keep_all || __tag_scan_matches(r, &params),
                    |__scan: &mut Vec<super::TagScanRef<'_>>| {
                        __tag_scan_sort(__scan, &qp.sort);
                    },
                    qp.pagination.offset,
                    qp.pagination.limit,
                    |__total: usize, __page: &[super::TagPageRef<'_>]| {
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
                    },
                );
        }
    };
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
    tag = "Tag",
    params(
        ("id" = String, Path, description = "Tag"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        )
    ),
    responses(
        (status = 200, description = "Get Tag by ID", body = Tag),
        (status = 404, description = "Not found")
    )
)]
async fn get_tag(
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                .into_response();
        }
    };
    let __as_of: Option<usize> = match params.get("as_of") {
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let __found = match __as_of {
        Some(__w) => db.tag.get_at(&forgedb_storage::Snapshot::new(__w), key),
        None => db.tag.get(key),
    };
    match __found {
        Some(record) => (StatusCode::OK, Json(record)).into_response(),
        None => {
            (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                .into_response()
        }
    }
}
#[utoipa::path(
    post,
    path = "",
    tag = "Tag",
    request_body = Tag,
    responses(
        (status = 201, description = "Create new Tag", body = Tag),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn create_tag(
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match serde_json::from_value::<super::Tag>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.create_tag(record) {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id" : id.to_string() }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Tag",
    params(("id" = String, Path, description = "Tag")),
    request_body = Tag,
    responses(
        (status = 200, description = "Replace Tag by ID", body = Tag),
        (status = 404, description = "Not found"),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn update_tag(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let record = match serde_json::from_value::<super::Tag>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.update_tag(key, record) {
        Ok(true) => (StatusCode::OK, Json(json!({ "id" : key.to_string() }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Tag",
    params(("id" = String, Path, description = "Tag")),
    responses(
        (status = 204, description = "Delete Tag by ID"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Referenced by children (on_delete=restrict)")
    )
)]
async fn delete_tag(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let mut db = db.write().await;
    match db.delete_tag(key) {
        Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::CONFLICT);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
fn tag_apply_sort(
    rows: &mut Vec<super::Tag>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "name" => rows.sort_by(|a, b| a.name.cmp(&b.name)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
/// Is this request unfiltered — does NO query key name a filterable field
/// of this model (#288)?
///
/// Hoisted out of the per-row loop by every caller, because it is a
/// property of the query string and not of the row. `_scan_matches` can
/// only short-circuit on `params.is_empty()`, and `?limit=50` defeats that
/// while naming no filterable field at all — so an unfiltered request was
/// running one `HashMap` lookup per filterable field per SCANNED ROW, all
/// of them guaranteed to miss. Measured at ~50 ns/row, i.e. 502 µs of a
/// 850 µs request over 10k rows.
///
/// The question is POSITIVE — "does any key name a field of this model?" —
/// deliberately. A negative exclusion list (`limit`/`offset`/`sort`/
/// `order`/`as_of`) would need maintaining, and would be wrong: a model may
/// legally declare a field named `limit`, and for that model `?limit=3`
/// genuinely is a filter.
fn __tag_is_unfiltered(params: &HashMap<String, String>) -> bool {
    if params.contains_key("id") {
        return false;
    }
    if params.contains_key("name") {
        return false;
    }
    true
}
/// Narrow closed-set filter over the BORROWED scan view (#160/#224), so
/// a row is accepted or rejected before its strings are ever copied out
/// of the buffered column.  Same per-field checks as `_event_matches`,
/// only the operand type is narrower.
fn __tag_scan_matches(
    record: &super::TagScanRef<'_>,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("name") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.name == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Narrow list sort over the borrowed scan view (#160/#228) — same arms
/// as `_apply_sort`.  Runs inside the scan scope, on views that borrow
/// the buffered columns: `&str` and `Option<&str>` are `Ord`, so
/// comparing them is comparing the buffer's bytes in place.
fn __tag_scan_sort(
    rows: &mut Vec<super::TagScanRef<'_>>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "name" => rows.sort_by(|a, b| a.name.cmp(&b.name)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
#[utoipa::path(
    get,
    path = "",
    tag = "Metric",
    params(
        (
            "limit" = Option<usize>,
            Query,
            description = "Max rows (clamped to [1, 1000]; default 50)"
        ),
        ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
        ("sort" = Option<String>, Query, description = "Field to sort by"),
        ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        ),
    ),
    responses((status = 200, description = "List all Metric", body = Vec<Metric>))
)]
async fn list_metric(
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
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let (page, total) = match __as_of {
        Some(__w) => {
            let mut rows: Vec<super::Metric> = db
                .metric
                .all_at(&forgedb_storage::Snapshot::new(__w))
                .into_iter()
                .filter(|r| metric_event_matches(r, &params))
                .collect();
            metric_apply_sort(&mut rows, &qp.sort);
            let total = rows.len();
            let page: Vec<super::Metric> = qp.pagination.apply(&rows).to_vec();
            (page, total)
        }
        None => {
            if params.get("projection").is_none() {
                let __keep_all: bool = __metric_is_unfiltered(&params);
                if __keep_all && qp.sort.is_none() {
                    return db
                        .metric
                        .__with_fast_page(
                            qp.pagination.offset,
                            qp.pagination.limit,
                            |__total: usize, __page: &[super::MetricPageRef<'_>]| {
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
                            },
                        );
                }
                let __sel: Option<Vec<usize>> = if let Some(__v) = params
                    .get("device_id")
                {
                    db.metric.__rows_by_device_id(__v)
                } else {
                    None
                };
                return db
                    .metric
                    .__with_page(
                        __sel,
                        |r| __keep_all || __metric_scan_matches(r, &params),
                        |__scan: &mut Vec<super::MetricScanRef<'_>>| {
                            __metric_scan_sort(__scan, &qp.sort);
                        },
                        qp.pagination.offset,
                        qp.pagination.limit,
                        |__total: usize, __page: &[super::MetricPageRef<'_>]| {
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
                        },
                    );
            }
            let __sel: Option<Vec<usize>> = if let Some(__v) = params.get("device_id") {
                db.metric.__rows_by_device_id(__v)
            } else {
                None
            };
            let __keep_all: bool = __metric_is_unfiltered(&params);
            let (total, __page_ids) = db
                .metric
                .__with_scan(
                    __sel,
                    |r| __keep_all || __metric_scan_matches(r, &params),
                    |__scan: &mut Vec<super::MetricScanRef<'_>>| {
                        __metric_scan_sort(__scan, &qp.sort);
                        let __total = __scan.len();
                        let __ids: Vec<_> = qp
                            .pagination
                            .apply(__scan)
                            .iter()
                            .map(|r| r.id)
                            .collect();
                        (__total, __ids)
                    },
                );
            let page: Vec<super::Metric> = __page_ids
                .iter()
                .filter_map(|__id| db.metric.get(*__id))
                .collect();
            (page, total)
        }
    };
    if let Some(__proj) = params.get("projection") {
        return match __proj.as_str() {
            "hot" => {
                let __data: Vec<super::MetricHot> = page
                    .iter()
                    .map(|r| super::MetricHot {
                        id: r.id.clone(),
                        cpu_pct: r.cpu_pct.clone(),
                        mem_pct: r.mem_pct.clone(),
                    })
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
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
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
    tag = "Metric",
    params(
        ("id" = String, Path, description = "Metric"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        )
    ),
    responses(
        (status = 200, description = "Get Metric by ID", body = Metric),
        (status = 404, description = "Not found")
    )
)]
async fn get_metric(
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if let Some(__proj) = params.get("projection") {
        let key = match id.parse::<Uuid>() {
            Ok(key) => key,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                    .into_response();
            }
        };
        let db = db.read().await;
        return match __proj.as_str() {
            "hot" => {
                match db.metric.get_hot(key) {
                    Some(r) => (StatusCode::OK, Json(r)).into_response(),
                    None => {
                        (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                            .into_response()
                    }
                }
            }
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                .into_response();
        }
    };
    let __as_of: Option<usize> = match params.get("as_of") {
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let __found = match __as_of {
        Some(__w) => db.metric.get_at(&forgedb_storage::Snapshot::new(__w), key),
        None => db.metric.get(key),
    };
    match __found {
        Some(record) => (StatusCode::OK, Json(record)).into_response(),
        None => {
            (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                .into_response()
        }
    }
}
#[utoipa::path(
    post,
    path = "",
    tag = "Metric",
    request_body = Metric,
    responses(
        (status = 201, description = "Create new Metric", body = Metric),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn create_metric(
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match serde_json::from_value::<super::Metric>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.create_metric(record) {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id" : id.to_string() }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Metric",
    params(("id" = String, Path, description = "Metric")),
    request_body = Metric,
    responses(
        (status = 200, description = "Replace Metric by ID", body = Metric),
        (status = 404, description = "Not found"),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn update_metric(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let record = match serde_json::from_value::<super::Metric>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.update_metric(key, record) {
        Ok(true) => (StatusCode::OK, Json(json!({ "id" : key.to_string() }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Metric",
    params(("id" = String, Path, description = "Metric")),
    responses(
        (status = 204, description = "Delete Metric by ID"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Referenced by children (on_delete=restrict)")
    )
)]
async fn delete_metric(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let mut db = db.write().await;
    match db.delete_metric(key) {
        Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::CONFLICT);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
fn metric_apply_sort(
    rows: &mut Vec<super::Metric>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "recorded_at" => rows.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at)),
        "device_id" => rows.sort_by(|a, b| a.device_id.cmp(&b.device_id)),
        "sample_seq" => rows.sort_by(|a, b| a.sample_seq.cmp(&b.sample_seq)),
        "region" => rows.sort_by(|a, b| a.region.cmp(&b.region)),
        "cpu_pct" => {
            rows.sort_by(|a, b| {
                a.cpu_pct.partial_cmp(&b.cpu_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "mem_pct" => {
            rows.sort_by(|a, b| {
                a.mem_pct.partial_cmp(&b.mem_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "disk_pct" => {
            rows.sort_by(|a, b| {
                a.disk_pct.partial_cmp(&b.disk_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "net_rx_bytes" => rows.sort_by(|a, b| a.net_rx_bytes.cmp(&b.net_rx_bytes)),
        "net_tx_bytes" => rows.sort_by(|a, b| a.net_tx_bytes.cmp(&b.net_tx_bytes)),
        "req_count" => rows.sort_by(|a, b| a.req_count.cmp(&b.req_count)),
        "err_count" => rows.sort_by(|a, b| a.err_count.cmp(&b.err_count)),
        "p50_micros" => rows.sort_by(|a, b| a.p50_micros.cmp(&b.p50_micros)),
        "p95_micros" => rows.sort_by(|a, b| a.p95_micros.cmp(&b.p95_micros)),
        "p99_micros" => rows.sort_by(|a, b| a.p99_micros.cmp(&b.p99_micros)),
        "queue_depth" => rows.sort_by(|a, b| a.queue_depth.cmp(&b.queue_depth)),
        "open_conns" => rows.sort_by(|a, b| a.open_conns.cmp(&b.open_conns)),
        "gc_pause_micros" => {
            rows.sort_by(|a, b| a.gc_pause_micros.cmp(&b.gc_pause_micros))
        }
        "uptime_secs" => rows.sort_by(|a, b| a.uptime_secs.cmp(&b.uptime_secs)),
        "temp_celsius" => {
            rows.sort_by(|a, b| {
                a.temp_celsius
                    .partial_cmp(&b.temp_celsius)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "throttled" => rows.sort_by(|a, b| a.throttled.cmp(&b.throttled)),
        "healthy" => rows.sort_by(|a, b| a.healthy.cmp(&b.healthy)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
/// Is this request unfiltered — does NO query key name a filterable field
/// of this model (#288)?
///
/// Hoisted out of the per-row loop by every caller, because it is a
/// property of the query string and not of the row. `_scan_matches` can
/// only short-circuit on `params.is_empty()`, and `?limit=50` defeats that
/// while naming no filterable field at all — so an unfiltered request was
/// running one `HashMap` lookup per filterable field per SCANNED ROW, all
/// of them guaranteed to miss. Measured at ~50 ns/row, i.e. 502 µs of a
/// 850 µs request over 10k rows.
///
/// The question is POSITIVE — "does any key name a field of this model?" —
/// deliberately. A negative exclusion list (`limit`/`offset`/`sort`/
/// `order`/`as_of`) would need maintaining, and would be wrong: a model may
/// legally declare a field named `limit`, and for that model `?limit=3`
/// genuinely is a filter.
fn __metric_is_unfiltered(params: &HashMap<String, String>) -> bool {
    if params.contains_key("id") {
        return false;
    }
    if params.contains_key("recorded_at") {
        return false;
    }
    if params.contains_key("device_id") {
        return false;
    }
    if params.contains_key("sample_seq") {
        return false;
    }
    if params.contains_key("region") {
        return false;
    }
    if params.contains_key("cpu_pct") {
        return false;
    }
    if params.contains_key("mem_pct") {
        return false;
    }
    if params.contains_key("disk_pct") {
        return false;
    }
    if params.contains_key("net_rx_bytes") {
        return false;
    }
    if params.contains_key("net_tx_bytes") {
        return false;
    }
    if params.contains_key("req_count") {
        return false;
    }
    if params.contains_key("err_count") {
        return false;
    }
    if params.contains_key("p50_micros") {
        return false;
    }
    if params.contains_key("p95_micros") {
        return false;
    }
    if params.contains_key("p99_micros") {
        return false;
    }
    if params.contains_key("queue_depth") {
        return false;
    }
    if params.contains_key("open_conns") {
        return false;
    }
    if params.contains_key("gc_pause_micros") {
        return false;
    }
    if params.contains_key("uptime_secs") {
        return false;
    }
    if params.contains_key("temp_celsius") {
        return false;
    }
    if params.contains_key("throttled") {
        return false;
    }
    if params.contains_key("healthy") {
        return false;
    }
    true
}
/// Narrow closed-set filter over the BORROWED scan view (#160/#224), so
/// a row is accepted or rejected before its strings are ever copied out
/// of the buffered column.  Same per-field checks as `_event_matches`,
/// only the operand type is narrower.
fn __metric_scan_matches(
    record: &super::MetricScanRef<'_>,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("recorded_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.recorded_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("device_id") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.device_id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("sample_seq") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.sample_seq == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("region") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.region == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("cpu_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.cpu_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("mem_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.mem_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("disk_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.disk_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("net_rx_bytes") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.net_rx_bytes == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("net_tx_bytes") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.net_tx_bytes == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("req_count") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.req_count == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("err_count") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.err_count == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p50_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p50_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p95_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p95_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p99_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p99_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("queue_depth") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.queue_depth == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("open_conns") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.open_conns == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("gc_pause_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.gc_pause_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("uptime_secs") {
        let __ok = match want.parse::<i64>().ok() {
            Some(__w) => record.uptime_secs == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("temp_celsius") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.temp_celsius == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("throttled") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.throttled == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("healthy") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.healthy == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Narrow list sort over the borrowed scan view (#160/#228) — same arms
/// as `_apply_sort`.  Runs inside the scan scope, on views that borrow
/// the buffered columns: `&str` and `Option<&str>` are `Ord`, so
/// comparing them is comparing the buffer's bytes in place.
fn __metric_scan_sort(
    rows: &mut Vec<super::MetricScanRef<'_>>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "recorded_at" => rows.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at)),
        "device_id" => rows.sort_by(|a, b| a.device_id.cmp(&b.device_id)),
        "sample_seq" => rows.sort_by(|a, b| a.sample_seq.cmp(&b.sample_seq)),
        "region" => rows.sort_by(|a, b| a.region.cmp(&b.region)),
        "cpu_pct" => {
            rows.sort_by(|a, b| {
                a.cpu_pct.partial_cmp(&b.cpu_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "mem_pct" => {
            rows.sort_by(|a, b| {
                a.mem_pct.partial_cmp(&b.mem_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "disk_pct" => {
            rows.sort_by(|a, b| {
                a.disk_pct.partial_cmp(&b.disk_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "net_rx_bytes" => rows.sort_by(|a, b| a.net_rx_bytes.cmp(&b.net_rx_bytes)),
        "net_tx_bytes" => rows.sort_by(|a, b| a.net_tx_bytes.cmp(&b.net_tx_bytes)),
        "req_count" => rows.sort_by(|a, b| a.req_count.cmp(&b.req_count)),
        "err_count" => rows.sort_by(|a, b| a.err_count.cmp(&b.err_count)),
        "p50_micros" => rows.sort_by(|a, b| a.p50_micros.cmp(&b.p50_micros)),
        "p95_micros" => rows.sort_by(|a, b| a.p95_micros.cmp(&b.p95_micros)),
        "p99_micros" => rows.sort_by(|a, b| a.p99_micros.cmp(&b.p99_micros)),
        "queue_depth" => rows.sort_by(|a, b| a.queue_depth.cmp(&b.queue_depth)),
        "open_conns" => rows.sort_by(|a, b| a.open_conns.cmp(&b.open_conns)),
        "gc_pause_micros" => {
            rows.sort_by(|a, b| a.gc_pause_micros.cmp(&b.gc_pause_micros))
        }
        "uptime_secs" => rows.sort_by(|a, b| a.uptime_secs.cmp(&b.uptime_secs)),
        "temp_celsius" => {
            rows.sort_by(|a, b| {
                a.temp_celsius
                    .partial_cmp(&b.temp_celsius)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        }
        "throttled" => rows.sort_by(|a, b| a.throttled.cmp(&b.throttled)),
        "healthy" => rows.sort_by(|a, b| a.healthy.cmp(&b.healthy)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
#[utoipa::path(
    get,
    path = "",
    tag = "Doc",
    params(
        (
            "limit" = Option<usize>,
            Query,
            description = "Max rows (clamped to [1, 1000]; default 50)"
        ),
        ("offset" = Option<usize>, Query, description = "Rows to skip (default 0)"),
        ("sort" = Option<String>, Query, description = "Field to sort by"),
        ("order" = Option<String>, Query, description = "asc | desc (default asc)"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        ),
    ),
    responses((status = 200, description = "List all Doc", body = Vec<Doc>))
)]
async fn list_doc(
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
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let (page, total) = match __as_of {
        Some(__w) => {
            let mut rows: Vec<super::Doc> = db
                .doc
                .all_at(&forgedb_storage::Snapshot::new(__w))
                .into_iter()
                .filter(|r| doc_event_matches(r, &params))
                .collect();
            doc_apply_sort(&mut rows, &qp.sort);
            let total = rows.len();
            let page: Vec<super::Doc> = qp.pagination.apply(&rows).to_vec();
            (page, total)
        }
        None => {
            if params.get("projection").is_none() {
                let __keep_all: bool = __doc_is_unfiltered(&params);
                if __keep_all && qp.sort.is_none() {
                    return db
                        .doc
                        .__with_fast_page(
                            qp.pagination.offset,
                            qp.pagination.limit,
                            |__total: usize, __page: &[super::DocPageRef<'_>]| {
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
                            },
                        );
                }
                let __sel: Option<Vec<usize>> = None;
                return db
                    .doc
                    .__with_page(
                        __sel,
                        |r| __keep_all || __doc_scan_matches(r, &params),
                        |__scan: &mut Vec<super::DocScanRef<'_>>| {
                            __doc_scan_sort(__scan, &qp.sort);
                        },
                        qp.pagination.offset,
                        qp.pagination.limit,
                        |__total: usize, __page: &[super::DocPageRef<'_>]| {
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
                        },
                    );
            }
            let __sel: Option<Vec<usize>> = None;
            let __keep_all: bool = __doc_is_unfiltered(&params);
            let (total, __page_ids) = db
                .doc
                .__with_scan(
                    __sel,
                    |r| __keep_all || __doc_scan_matches(r, &params),
                    |__scan: &mut Vec<super::DocScanRef<'_>>| {
                        __doc_scan_sort(__scan, &qp.sort);
                        let __total = __scan.len();
                        let __ids: Vec<_> = qp
                            .pagination
                            .apply(__scan)
                            .iter()
                            .map(|r| r.id)
                            .collect();
                        (__total, __ids)
                    },
                );
            let page: Vec<super::Doc> = __page_ids
                .iter()
                .filter_map(|__id| db.doc.get(*__id))
                .collect();
            (page, total)
        }
    };
    if let Some(__proj) = params.get("projection") {
        return match __proj.as_str() {
            "meta" => {
                let __data: Vec<super::DocMeta> = page
                    .iter()
                    .map(|r| super::DocMeta {
                        id: r.id.clone(),
                        seq: r.seq.clone(),
                        kind: r.kind.clone(),
                    })
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
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
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
    tag = "Doc",
    params(
        ("id" = String, Path, description = "Doc"),
        (
            "as_of" = Option<usize>,
            Query,
            description = "Row-count watermark for a point-in-time read (#85); non-numeric → 400"
        )
    ),
    responses(
        (status = 200, description = "Get Doc by ID", body = Doc),
        (status = 404, description = "Not found")
    )
)]
async fn get_doc(
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if let Some(__proj) = params.get("projection") {
        let key = match id.parse::<Uuid>() {
            Ok(key) => key,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                    .into_response();
            }
        };
        let db = db.read().await;
        return match __proj.as_str() {
            "meta" => {
                match db.doc.get_meta(key) {
                    Some(r) => (StatusCode::OK, Json(r)).into_response(),
                    None => {
                        (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                            .into_response()
                    }
                }
            }
            _ => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error" : "unknown projection" })),
                )
                    .into_response()
            }
        };
    }
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })))
                .into_response();
        }
    };
    let __as_of: Option<usize> = match params.get("as_of") {
        Some(__w) => {
            match __w.parse::<usize>() {
                Ok(__n) => Some(__n),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!(
                                { "error" : "as_of must be a non-negative integer watermark"
                                }
                            ),
                        ),
                    )
                        .into_response();
                }
            }
        }
        None => None,
    };
    let db = db.read().await;
    let __found = match __as_of {
        Some(__w) => db.doc.get_at(&forgedb_storage::Snapshot::new(__w), key),
        None => db.doc.get(key),
    };
    match __found {
        Some(record) => (StatusCode::OK, Json(record)).into_response(),
        None => {
            (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" })))
                .into_response()
        }
    }
}
#[utoipa::path(
    post,
    path = "",
    tag = "Doc",
    request_body = Doc,
    responses(
        (status = 201, description = "Create new Doc", body = Doc),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn create_doc(
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match serde_json::from_value::<super::Doc>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.create_doc(record) {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id" : id.to_string() }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Doc",
    params(("id" = String, Path, description = "Doc")),
    request_body = Doc,
    responses(
        (status = 200, description = "Replace Doc by ID", body = Doc),
        (status = 404, description = "Not found"),
        (
            status = 409,
            description = "Integrity conflict (duplicate unique / dangling foreign key)"
        ),
        (status = 422, description = "Field constraint violation")
    )
)]
async fn update_doc(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let record = match serde_json::from_value::<super::Doc>(payload) {
        Ok(record) => record,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error" : "invalid payload" })),
            );
        }
    };
    let mut db = db.write().await;
    match db.update_doc(key, record) {
        Ok(true) => (StatusCode::OK, Json(json!({ "id" : key.to_string() }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Doc",
    params(("id" = String, Path, description = "Doc")),
    responses(
        (status = 204, description = "Delete Doc by ID"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Referenced by children (on_delete=restrict)")
    )
)]
async fn delete_doc(
    Path(id): Path<String>,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let key = match id.parse::<Uuid>() {
        Ok(key) => key,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error" : "invalid id" })));
        }
    };
    let mut db = db.write().await;
    match db.delete_doc(key) {
        Ok(true) => (StatusCode::NO_CONTENT, Json(json!({}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error" : "not found" }))),
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::CONFLICT);
            (status, Json(json!({ "error" : e.to_string() })))
        }
    }
}
fn doc_apply_sort(
    rows: &mut Vec<super::Doc>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "seq" => rows.sort_by(|a, b| a.seq.cmp(&b.seq)),
        "kind" => rows.sort_by(|a, b| a.kind.cmp(&b.kind)),
        "body_a" => rows.sort_by(|a, b| a.body_a.cmp(&b.body_a)),
        "body_b" => rows.sort_by(|a, b| a.body_b.cmp(&b.body_b)),
        "body_c" => rows.sort_by(|a, b| a.body_c.cmp(&b.body_c)),
        "body_d" => rows.sort_by(|a, b| a.body_d.cmp(&b.body_d)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
/// Is this request unfiltered — does NO query key name a filterable field
/// of this model (#288)?
///
/// Hoisted out of the per-row loop by every caller, because it is a
/// property of the query string and not of the row. `_scan_matches` can
/// only short-circuit on `params.is_empty()`, and `?limit=50` defeats that
/// while naming no filterable field at all — so an unfiltered request was
/// running one `HashMap` lookup per filterable field per SCANNED ROW, all
/// of them guaranteed to miss. Measured at ~50 ns/row, i.e. 502 µs of a
/// 850 µs request over 10k rows.
///
/// The question is POSITIVE — "does any key name a field of this model?" —
/// deliberately. A negative exclusion list (`limit`/`offset`/`sort`/
/// `order`/`as_of`) would need maintaining, and would be wrong: a model may
/// legally declare a field named `limit`, and for that model `?limit=3`
/// genuinely is a filter.
fn __doc_is_unfiltered(params: &HashMap<String, String>) -> bool {
    if params.contains_key("id") {
        return false;
    }
    if params.contains_key("seq") {
        return false;
    }
    if params.contains_key("kind") {
        return false;
    }
    if params.contains_key("body_a") {
        return false;
    }
    if params.contains_key("body_b") {
        return false;
    }
    if params.contains_key("body_c") {
        return false;
    }
    if params.contains_key("body_d") {
        return false;
    }
    true
}
/// Narrow closed-set filter over the BORROWED scan view (#160/#224), so
/// a row is accepted or rejected before its strings are ever copied out
/// of the buffered column.  Same per-field checks as `_event_matches`,
/// only the operand type is narrower.
fn __doc_scan_matches(
    record: &super::DocScanRef<'_>,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("seq") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.seq == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("kind") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.kind == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_a") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_a == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_b") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_b == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_c") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_c == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_d") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_d == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Narrow list sort over the borrowed scan view (#160/#228) — same arms
/// as `_apply_sort`.  Runs inside the scan scope, on views that borrow
/// the buffered columns: `&str` and `Option<&str>` are `Ord`, so
/// comparing them is comparing the buffer's bytes in place.
fn __doc_scan_sort(
    rows: &mut Vec<super::DocScanRef<'_>>,
    sort: &Option<forgedb_query_params::Sort>,
) {
    let Some(sort) = sort.as_ref() else {
        return;
    };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "seq" => rows.sort_by(|a, b| a.seq.cmp(&b.seq)),
        "kind" => rows.sort_by(|a, b| a.kind.cmp(&b.kind)),
        "body_a" => rows.sort_by(|a, b| a.body_a.cmp(&b.body_a)),
        "body_b" => rows.sort_by(|a, b| a.body_b.cmp(&b.body_b)),
        "body_c" => rows.sort_by(|a, b| a.body_c.cmp(&b.body_c)),
        "body_d" => rows.sort_by(|a, b| a.body_d.cmp(&b.body_d)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}
///Per-model change-feed filter for `User` (#62): narrow by exact-match `?field=value` query params. Each declared scalar field is checked by name in generated code, parsing the param into the field's type and comparing typed values (#84 — `?n=3` matches a stored `3.0`); the substrate feed never inspects a field. An empty param set matches everything; unknown keys are ignored.
fn user_event_matches(record: &super::User, params: &HashMap<String, String>) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("name") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.name == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("email") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.email == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("created_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.created_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Typed per-field change detector for the live-query `Updated` diff (#84).
fn user_record_changed(a: &super::User, b: &super::User) -> bool {
    if a.id != b.id {
        return true;
    }
    if a.name != b.name {
        return true;
    }
    if a.email != b.email {
        return true;
    }
    if a.created_at != b.created_at {
        return true;
    }
    false
}
///WebSocket subscription for `User` changes (#62 Direction A + #66). Upgrades the connection and streams a typed `UserInserted` / `UserUpdated` / `UserDeleted` JSON event per change, optionally narrowed by `?field=value`.
async fn subscribe_user(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_user_subscription(socket, db, params))
}
async fn handle_user_subscription(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let mut rx = { db.read().await.changefeed.subscribe() };
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "User" {
                    continue;
                }
                let record = { db.read().await.user.read_at(event.row_index) };
                let Some(record) = record else {
                    continue;
                };
                if !user_event_matches(&record, &params) {
                    continue;
                }
                let text = match event.kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        serde_json::to_string(
                            &super::UserInserted {
                                user: record,
                            },
                        )
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        serde_json::to_string(&super::UserUpdated { user: record })
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        serde_json::to_string(&super::UserDeleted { user: record })
                    }
                    forgedb_changefeed::ChangeKind::Linked => continue,
                };
                let Ok(text) = text else {
                    continue;
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
///Per-model change-feed filter for `Post` (#62): narrow by exact-match `?field=value` query params. Each declared scalar field is checked by name in generated code, parsing the param into the field's type and comparing typed values (#84 — `?n=3` matches a stored `3.0`); the substrate feed never inspects a field. An empty param set matches everything; unknown keys are ignored.
fn post_event_matches(record: &super::Post, params: &HashMap<String, String>) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("title") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.title == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("views") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.views == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("published") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.published == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("created_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.created_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Typed per-field change detector for the live-query `Updated` diff (#84).
fn post_record_changed(a: &super::Post, b: &super::Post) -> bool {
    if a.id != b.id {
        return true;
    }
    if a.title != b.title {
        return true;
    }
    if a.views != b.views {
        return true;
    }
    if a.published != b.published {
        return true;
    }
    if a.author != b.author {
        return true;
    }
    if a.created_at != b.created_at {
        return true;
    }
    false
}
///WebSocket subscription for `Post` changes (#62 Direction A + #66). Upgrades the connection and streams a typed `PostInserted` / `PostUpdated` / `PostDeleted` JSON event per change, optionally narrowed by `?field=value`.
async fn subscribe_post(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_post_subscription(socket, db, params))
}
async fn handle_post_subscription(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let mut rx = { db.read().await.changefeed.subscribe() };
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Post" {
                    continue;
                }
                let record = { db.read().await.post.read_at(event.row_index) };
                let Some(record) = record else {
                    continue;
                };
                if !post_event_matches(&record, &params) {
                    continue;
                }
                let text = match event.kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        serde_json::to_string(
                            &super::PostInserted {
                                post: record,
                            },
                        )
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        serde_json::to_string(&super::PostUpdated { post: record })
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        serde_json::to_string(&super::PostDeleted { post: record })
                    }
                    forgedb_changefeed::ChangeKind::Linked => continue,
                };
                let Ok(text) = text else {
                    continue;
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
///Per-model change-feed filter for `Tag` (#62): narrow by exact-match `?field=value` query params. Each declared scalar field is checked by name in generated code, parsing the param into the field's type and comparing typed values (#84 — `?n=3` matches a stored `3.0`); the substrate feed never inspects a field. An empty param set matches everything; unknown keys are ignored.
fn tag_event_matches(record: &super::Tag, params: &HashMap<String, String>) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("name") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.name == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Typed per-field change detector for the live-query `Updated` diff (#84).
fn tag_record_changed(a: &super::Tag, b: &super::Tag) -> bool {
    if a.id != b.id {
        return true;
    }
    if a.name != b.name {
        return true;
    }
    false
}
///WebSocket subscription for `Tag` changes (#62 Direction A + #66). Upgrades the connection and streams a typed `TagInserted` / `TagUpdated` / `TagDeleted` JSON event per change, optionally narrowed by `?field=value`.
async fn subscribe_tag(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_tag_subscription(socket, db, params))
}
async fn handle_tag_subscription(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let mut rx = { db.read().await.changefeed.subscribe() };
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Tag" {
                    continue;
                }
                let record = { db.read().await.tag.read_at(event.row_index) };
                let Some(record) = record else {
                    continue;
                };
                if !tag_event_matches(&record, &params) {
                    continue;
                }
                let text = match event.kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        serde_json::to_string(&super::TagInserted { tag: record })
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        serde_json::to_string(&super::TagUpdated { tag: record })
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        serde_json::to_string(&super::TagDeleted { tag: record })
                    }
                    forgedb_changefeed::ChangeKind::Linked => continue,
                };
                let Ok(text) = text else {
                    continue;
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
///Per-model change-feed filter for `Metric` (#62): narrow by exact-match `?field=value` query params. Each declared scalar field is checked by name in generated code, parsing the param into the field's type and comparing typed values (#84 — `?n=3` matches a stored `3.0`); the substrate feed never inspects a field. An empty param set matches everything; unknown keys are ignored.
fn metric_event_matches(
    record: &super::Metric,
    params: &HashMap<String, String>,
) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("recorded_at") {
        let __ok = match want.parse::<forgedb_types::Timestamp>().ok() {
            Some(__w) => record.recorded_at == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("device_id") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.device_id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("sample_seq") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.sample_seq == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("region") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.region == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("cpu_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.cpu_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("mem_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.mem_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("disk_pct") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.disk_pct == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("net_rx_bytes") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.net_rx_bytes == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("net_tx_bytes") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.net_tx_bytes == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("req_count") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.req_count == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("err_count") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.err_count == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p50_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p50_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p95_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p95_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("p99_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.p99_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("queue_depth") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.queue_depth == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("open_conns") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.open_conns == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("gc_pause_micros") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.gc_pause_micros == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("uptime_secs") {
        let __ok = match want.parse::<i64>().ok() {
            Some(__w) => record.uptime_secs == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("temp_celsius") {
        let __ok = match want.parse::<f64>().ok() {
            Some(__w) => record.temp_celsius == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("throttled") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.throttled == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("healthy") {
        let __ok = match want.parse::<bool>().ok() {
            Some(__w) => record.healthy == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Typed per-field change detector for the live-query `Updated` diff (#84).
fn metric_record_changed(a: &super::Metric, b: &super::Metric) -> bool {
    if a.id != b.id {
        return true;
    }
    if a.recorded_at != b.recorded_at {
        return true;
    }
    if a.device_id != b.device_id {
        return true;
    }
    if a.sample_seq != b.sample_seq {
        return true;
    }
    if a.region != b.region {
        return true;
    }
    if a.cpu_pct.to_bits() != b.cpu_pct.to_bits() {
        return true;
    }
    if a.mem_pct.to_bits() != b.mem_pct.to_bits() {
        return true;
    }
    if a.disk_pct.to_bits() != b.disk_pct.to_bits() {
        return true;
    }
    if a.net_rx_bytes != b.net_rx_bytes {
        return true;
    }
    if a.net_tx_bytes != b.net_tx_bytes {
        return true;
    }
    if a.req_count != b.req_count {
        return true;
    }
    if a.err_count != b.err_count {
        return true;
    }
    if a.p50_micros != b.p50_micros {
        return true;
    }
    if a.p95_micros != b.p95_micros {
        return true;
    }
    if a.p99_micros != b.p99_micros {
        return true;
    }
    if a.queue_depth != b.queue_depth {
        return true;
    }
    if a.open_conns != b.open_conns {
        return true;
    }
    if a.gc_pause_micros != b.gc_pause_micros {
        return true;
    }
    if a.uptime_secs != b.uptime_secs {
        return true;
    }
    if a.temp_celsius.to_bits() != b.temp_celsius.to_bits() {
        return true;
    }
    if a.throttled != b.throttled {
        return true;
    }
    if a.healthy != b.healthy {
        return true;
    }
    false
}
///WebSocket subscription for `Metric` changes (#62 Direction A + #66). Upgrades the connection and streams a typed `MetricInserted` / `MetricUpdated` / `MetricDeleted` JSON event per change, optionally narrowed by `?field=value`.
async fn subscribe_metric(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_metric_subscription(socket, db, params))
}
async fn handle_metric_subscription(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let mut rx = { db.read().await.changefeed.subscribe() };
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Metric" {
                    continue;
                }
                let record = { db.read().await.metric.read_at(event.row_index) };
                let Some(record) = record else {
                    continue;
                };
                if !metric_event_matches(&record, &params) {
                    continue;
                }
                let text = match event.kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        serde_json::to_string(
                            &super::MetricInserted {
                                metric: record,
                            },
                        )
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        serde_json::to_string(
                            &super::MetricUpdated {
                                metric: record,
                            },
                        )
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        serde_json::to_string(
                            &super::MetricDeleted {
                                metric: record,
                            },
                        )
                    }
                    forgedb_changefeed::ChangeKind::Linked => continue,
                };
                let Ok(text) = text else {
                    continue;
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
///Per-model change-feed filter for `Doc` (#62): narrow by exact-match `?field=value` query params. Each declared scalar field is checked by name in generated code, parsing the param into the field's type and comparing typed values (#84 — `?n=3` matches a stored `3.0`); the substrate feed never inspects a field. An empty param set matches everything; unknown keys are ignored.
fn doc_event_matches(record: &super::Doc, params: &HashMap<String, String>) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("id") {
        let __ok = match want.parse::<Uuid>().ok() {
            Some(__w) => record.id == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("seq") {
        let __ok = match want.parse::<u64>().ok() {
            Some(__w) => record.seq == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("kind") {
        let __ok = match want.parse::<u32>().ok() {
            Some(__w) => record.kind == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_a") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_a == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_b") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_b == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_c") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_c == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    if let Some(want) = params.get("body_d") {
        let __ok = match Some(want.clone()) {
            Some(__w) => record.body_d == __w,
            None => false,
        };
        if !__ok {
            return false;
        }
    }
    true
}
/// Typed per-field change detector for the live-query `Updated` diff (#84).
fn doc_record_changed(a: &super::Doc, b: &super::Doc) -> bool {
    if a.id != b.id {
        return true;
    }
    if a.seq != b.seq {
        return true;
    }
    if a.kind != b.kind {
        return true;
    }
    if a.body_a != b.body_a {
        return true;
    }
    if a.body_b != b.body_b {
        return true;
    }
    if a.body_c != b.body_c {
        return true;
    }
    if a.body_d != b.body_d {
        return true;
    }
    false
}
///WebSocket subscription for `Doc` changes (#62 Direction A + #66). Upgrades the connection and streams a typed `DocInserted` / `DocUpdated` / `DocDeleted` JSON event per change, optionally narrowed by `?field=value`.
async fn subscribe_doc(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_doc_subscription(socket, db, params))
}
async fn handle_doc_subscription(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let mut rx = { db.read().await.changefeed.subscribe() };
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Doc" {
                    continue;
                }
                let record = { db.read().await.doc.read_at(event.row_index) };
                let Some(record) = record else {
                    continue;
                };
                if !doc_event_matches(&record, &params) {
                    continue;
                }
                let text = match event.kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        serde_json::to_string(&super::DocInserted { doc: record })
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        serde_json::to_string(&super::DocUpdated { doc: record })
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        serde_json::to_string(&super::DocDeleted { doc: record })
                    }
                    forgedb_changefeed::ChangeKind::Linked => continue,
                };
                let Ok(text) = text else {
                    continue;
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
///Live-query WebSocket subscription for `User` (#62 Direction B). Runs the generated closed-set query (narrow `__with_scan` + `__user_scan_matches`, materializing only matches — #160), streams an initial `UserLiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / `Removed` deltas as the matching set changes.
async fn subscribe_live_user(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_user_live_query(socket, db, params))
}
async fn handle_user_live_query(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let __keep_all: bool = __user_is_unfiltered(&params);
    let mut rx = { db.read().await.changefeed.subscribe() };
    let mut members: HashMap<Uuid, super::User> = HashMap::new();
    {
        let rows: Vec<super::User> = {
            let g = db.read().await;
            let __ids = g
                .user
                .__with_scan(
                    None,
                    |r| __keep_all || __user_scan_matches(r, &params),
                    |__scan: &mut Vec<super::UserScanRef<'_>>| {
                        __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                    },
                );
            __ids.into_iter().filter_map(|__id| g.user.get(__id)).collect()
        };
        for r in &rows {
            members.insert(r.id, r.clone());
        }
        let init = super::UserLiveDelta::Init { rows };
        if let Ok(text) = serde_json::to_string(&init) {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "User" {
                    continue;
                }
                let current: Vec<super::User> = {
                    let g = db.read().await;
                    let __ids = g
                        .user
                        .__with_scan(
                            None,
                            |r| __keep_all || __user_scan_matches(r, &params),
                            |__scan: &mut Vec<super::UserScanRef<'_>>| {
                                __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                            },
                        );
                    __ids.into_iter().filter_map(|__id| g.user.get(__id)).collect()
                };
                let mut next: HashMap<Uuid, super::User> = HashMap::new();
                let mut deltas: Vec<super::UserLiveDelta> = Vec::new();
                for r in current {
                    let id = r.id;
                    match members.get(&id) {
                        None => {
                            deltas
                                .push(super::UserLiveDelta::Added {
                                    row: r.clone(),
                                })
                        }
                        Some(prev) if user_record_changed(prev, &r) => {
                            deltas
                                .push(super::UserLiveDelta::Updated {
                                    row: r.clone(),
                                })
                        }
                        _ => {}
                    }
                    next.insert(id, r);
                }
                for id in members.keys() {
                    if !next.contains_key(id) {
                        deltas
                            .push(super::UserLiveDelta::Removed {
                                id: *id,
                            });
                    }
                }
                members = next;
                for d in deltas {
                    let Ok(text) = serde_json::to_string(&d) else {
                        continue;
                    };
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
///Live-query WebSocket subscription for `Post` (#62 Direction B). Runs the generated closed-set query (narrow `__with_scan` + `__post_scan_matches`, materializing only matches — #160), streams an initial `PostLiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / `Removed` deltas as the matching set changes.
async fn subscribe_live_post(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_post_live_query(socket, db, params))
}
async fn handle_post_live_query(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let __keep_all: bool = __post_is_unfiltered(&params);
    let mut rx = { db.read().await.changefeed.subscribe() };
    let mut members: HashMap<Uuid, super::Post> = HashMap::new();
    {
        let rows: Vec<super::Post> = {
            let g = db.read().await;
            let __ids = g
                .post
                .__with_scan(
                    None,
                    |r| __keep_all || __post_scan_matches(r, &params),
                    |__scan: &mut Vec<super::PostScanRef<'_>>| {
                        __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                    },
                );
            __ids.into_iter().filter_map(|__id| g.post.get(__id)).collect()
        };
        for r in &rows {
            members.insert(r.id, r.clone());
        }
        let init = super::PostLiveDelta::Init { rows };
        if let Ok(text) = serde_json::to_string(&init) {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Post" {
                    continue;
                }
                let current: Vec<super::Post> = {
                    let g = db.read().await;
                    let __ids = g
                        .post
                        .__with_scan(
                            None,
                            |r| __keep_all || __post_scan_matches(r, &params),
                            |__scan: &mut Vec<super::PostScanRef<'_>>| {
                                __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                            },
                        );
                    __ids.into_iter().filter_map(|__id| g.post.get(__id)).collect()
                };
                let mut next: HashMap<Uuid, super::Post> = HashMap::new();
                let mut deltas: Vec<super::PostLiveDelta> = Vec::new();
                for r in current {
                    let id = r.id;
                    match members.get(&id) {
                        None => {
                            deltas
                                .push(super::PostLiveDelta::Added {
                                    row: r.clone(),
                                })
                        }
                        Some(prev) if post_record_changed(prev, &r) => {
                            deltas
                                .push(super::PostLiveDelta::Updated {
                                    row: r.clone(),
                                })
                        }
                        _ => {}
                    }
                    next.insert(id, r);
                }
                for id in members.keys() {
                    if !next.contains_key(id) {
                        deltas
                            .push(super::PostLiveDelta::Removed {
                                id: *id,
                            });
                    }
                }
                members = next;
                for d in deltas {
                    let Ok(text) = serde_json::to_string(&d) else {
                        continue;
                    };
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
///Live-query WebSocket subscription for `Tag` (#62 Direction B). Runs the generated closed-set query (narrow `__with_scan` + `__tag_scan_matches`, materializing only matches — #160), streams an initial `TagLiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / `Removed` deltas as the matching set changes.
async fn subscribe_live_tag(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_tag_live_query(socket, db, params))
}
async fn handle_tag_live_query(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let __keep_all: bool = __tag_is_unfiltered(&params);
    let mut rx = { db.read().await.changefeed.subscribe() };
    let mut members: HashMap<Uuid, super::Tag> = HashMap::new();
    {
        let rows: Vec<super::Tag> = {
            let g = db.read().await;
            let __ids = g
                .tag
                .__with_scan(
                    None,
                    |r| __keep_all || __tag_scan_matches(r, &params),
                    |__scan: &mut Vec<super::TagScanRef<'_>>| {
                        __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                    },
                );
            __ids.into_iter().filter_map(|__id| g.tag.get(__id)).collect()
        };
        for r in &rows {
            members.insert(r.id, r.clone());
        }
        let init = super::TagLiveDelta::Init { rows };
        if let Ok(text) = serde_json::to_string(&init) {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Tag" {
                    continue;
                }
                let current: Vec<super::Tag> = {
                    let g = db.read().await;
                    let __ids = g
                        .tag
                        .__with_scan(
                            None,
                            |r| __keep_all || __tag_scan_matches(r, &params),
                            |__scan: &mut Vec<super::TagScanRef<'_>>| {
                                __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                            },
                        );
                    __ids.into_iter().filter_map(|__id| g.tag.get(__id)).collect()
                };
                let mut next: HashMap<Uuid, super::Tag> = HashMap::new();
                let mut deltas: Vec<super::TagLiveDelta> = Vec::new();
                for r in current {
                    let id = r.id;
                    match members.get(&id) {
                        None => {
                            deltas
                                .push(super::TagLiveDelta::Added {
                                    row: r.clone(),
                                })
                        }
                        Some(prev) if tag_record_changed(prev, &r) => {
                            deltas
                                .push(super::TagLiveDelta::Updated {
                                    row: r.clone(),
                                })
                        }
                        _ => {}
                    }
                    next.insert(id, r);
                }
                for id in members.keys() {
                    if !next.contains_key(id) {
                        deltas
                            .push(super::TagLiveDelta::Removed {
                                id: *id,
                            });
                    }
                }
                members = next;
                for d in deltas {
                    let Ok(text) = serde_json::to_string(&d) else {
                        continue;
                    };
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
///Live-query WebSocket subscription for `Metric` (#62 Direction B). Runs the generated closed-set query (narrow `__with_scan` + `__metric_scan_matches`, materializing only matches — #160), streams an initial `MetricLiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / `Removed` deltas as the matching set changes.
async fn subscribe_live_metric(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_metric_live_query(socket, db, params))
}
async fn handle_metric_live_query(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let __keep_all: bool = __metric_is_unfiltered(&params);
    let mut rx = { db.read().await.changefeed.subscribe() };
    let mut members: HashMap<Uuid, super::Metric> = HashMap::new();
    {
        let rows: Vec<super::Metric> = {
            let g = db.read().await;
            let __ids = g
                .metric
                .__with_scan(
                    None,
                    |r| __keep_all || __metric_scan_matches(r, &params),
                    |__scan: &mut Vec<super::MetricScanRef<'_>>| {
                        __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                    },
                );
            __ids.into_iter().filter_map(|__id| g.metric.get(__id)).collect()
        };
        for r in &rows {
            members.insert(r.id, r.clone());
        }
        let init = super::MetricLiveDelta::Init {
            rows,
        };
        if let Ok(text) = serde_json::to_string(&init) {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Metric" {
                    continue;
                }
                let current: Vec<super::Metric> = {
                    let g = db.read().await;
                    let __ids = g
                        .metric
                        .__with_scan(
                            None,
                            |r| __keep_all || __metric_scan_matches(r, &params),
                            |__scan: &mut Vec<super::MetricScanRef<'_>>| {
                                __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                            },
                        );
                    __ids.into_iter().filter_map(|__id| g.metric.get(__id)).collect()
                };
                let mut next: HashMap<Uuid, super::Metric> = HashMap::new();
                let mut deltas: Vec<super::MetricLiveDelta> = Vec::new();
                for r in current {
                    let id = r.id;
                    match members.get(&id) {
                        None => {
                            deltas
                                .push(super::MetricLiveDelta::Added {
                                    row: r.clone(),
                                })
                        }
                        Some(prev) if metric_record_changed(prev, &r) => {
                            deltas
                                .push(super::MetricLiveDelta::Updated {
                                    row: r.clone(),
                                })
                        }
                        _ => {}
                    }
                    next.insert(id, r);
                }
                for id in members.keys() {
                    if !next.contains_key(id) {
                        deltas
                            .push(super::MetricLiveDelta::Removed {
                                id: *id,
                            });
                    }
                }
                members = next;
                for d in deltas {
                    let Ok(text) = serde_json::to_string(&d) else {
                        continue;
                    };
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
///Live-query WebSocket subscription for `Doc` (#62 Direction B). Runs the generated closed-set query (narrow `__with_scan` + `__doc_scan_matches`, materializing only matches — #160), streams an initial `DocLiveDelta::Init`, then pushes removal-aware `Added` / `Updated` / `Removed` deltas as the matching set changes.
async fn subscribe_live_doc(
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    axum::Extension(allowed): axum::Extension<AllowedOrigins>,
    ws: WebSocketUpgrade,
    State(db): State<Arc<RwLock<super::Database>>>,
) -> Response {
    if !allowed.permits(__origin_of(&headers)) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.on_upgrade(move |socket| handle_doc_live_query(socket, db, params))
}
async fn handle_doc_live_query(
    mut socket: WebSocket,
    db: Arc<RwLock<super::Database>>,
    params: HashMap<String, String>,
) {
    let __keep_all: bool = __doc_is_unfiltered(&params);
    let mut rx = { db.read().await.changefeed.subscribe() };
    let mut members: HashMap<Uuid, super::Doc> = HashMap::new();
    {
        let rows: Vec<super::Doc> = {
            let g = db.read().await;
            let __ids = g
                .doc
                .__with_scan(
                    None,
                    |r| __keep_all || __doc_scan_matches(r, &params),
                    |__scan: &mut Vec<super::DocScanRef<'_>>| {
                        __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                    },
                );
            __ids.into_iter().filter_map(|__id| g.doc.get(__id)).collect()
        };
        for r in &rows {
            members.insert(r.id, r.clone());
        }
        let init = super::DocLiveDelta::Init { rows };
        if let Ok(text) = serde_json::to_string(&init) {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.model != "Doc" {
                    continue;
                }
                let current: Vec<super::Doc> = {
                    let g = db.read().await;
                    let __ids = g
                        .doc
                        .__with_scan(
                            None,
                            |r| __keep_all || __doc_scan_matches(r, &params),
                            |__scan: &mut Vec<super::DocScanRef<'_>>| {
                                __scan.iter().map(|r| r.id).collect::<Vec<_>>()
                            },
                        );
                    __ids.into_iter().filter_map(|__id| g.doc.get(__id)).collect()
                };
                let mut next: HashMap<Uuid, super::Doc> = HashMap::new();
                let mut deltas: Vec<super::DocLiveDelta> = Vec::new();
                for r in current {
                    let id = r.id;
                    match members.get(&id) {
                        None => {
                            deltas
                                .push(super::DocLiveDelta::Added {
                                    row: r.clone(),
                                })
                        }
                        Some(prev) if doc_record_changed(prev, &r) => {
                            deltas
                                .push(super::DocLiveDelta::Updated {
                                    row: r.clone(),
                                })
                        }
                        _ => {}
                    }
                    next.insert(id, r);
                }
                for id in members.keys() {
                    if !next.contains_key(id) {
                        deltas
                            .push(super::DocLiveDelta::Removed {
                                id: *id,
                            });
                    }
                }
                members = next;
                for d in deltas {
                    let Ok(text) = serde_json::to_string(&d) else {
                        continue;
                    };
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
/// Upgrade to a replication stream.  `?after=<offset>` resumes from the
/// follower's last applied offset (default `0` = cold / from the start
/// of the retained log).  Tenant-scoped by the router's auth guard.
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
    let after: u64 = params.get("after").and_then(|s| s.parse().ok()).unwrap_or(0);
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
        if socket.send(Message::Binary(ev.to_wire().into())).await.is_err() {
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
                if socket.send(Message::Binary(ev.to_wire().into())).await.is_err() {
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
/// Liveness probe (Phase 5): 200 as long as the process is up and
/// the async runtime is scheduling.  Never touches the database, so it
/// never blocks on a write lock — the correct signal for a k8s
/// `livenessProbe` / load-balancer health check.
async fn __health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(json!({ "status" : "ok" })))
}
/// Readiness probe (Phase 5): acquires a read lock on the database
/// and returns 200 once obtained, proving the store opened and the
/// lock is not wedged — the correct signal for a k8s `readinessProbe`.
async fn __ready(
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let _guard = db.read().await;
    (StatusCode::OK, Json(json!({ "status" : "ready" })))
}
/// Minimal metrics (Phase 5): per-model live row counts + totals,
/// as JSON.  Generated per-schema by naming each model's storage field;
/// no schema is interpreted at runtime.
async fn __metrics(
    State(db): State<Arc<RwLock<super::Database>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let db = db.read().await;
    let per_model = json!(
        { "User" : db.user.row_count(), "Post" : db.post.row_count(), "Tag" : db.tag
        .row_count(), "Metric" : db.metric.row_count(), "Doc" : db.doc.row_count(), }
    );
    let total_rows: usize = 0 + db.user.row_count() + db.post.row_count()
        + db.tag.row_count() + db.metric.row_count() + db.doc.row_count();
    let body = json!(
        { "model_count" : 5usize, "total_rows" : total_rows, "rows_per_model" :
        per_model, }
    );
    (StatusCode::OK, Json(body))
}
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
    let watermarks = json!(
        { "User" : db.user.row_count(), "Post" : db.post.row_count(), "Tag" : db.tag
        .row_count(), "Metric" : db.metric.row_count(), "Doc" : db.doc.row_count(), }
    );
    (StatusCode::OK, Json(json!({ "watermarks" : watermarks })))
}
#[derive(OpenApi)]
#[openapi(
    paths(
        list_user,
        list_post,
        list_tag,
        list_metric,
        list_doc,
        get_user,
        get_post,
        get_tag,
        get_metric,
        get_doc,
        create_user,
        create_post,
        create_tag,
        create_metric,
        create_doc,
        update_user,
        update_post,
        update_tag,
        update_metric,
        update_doc,
        delete_user,
        delete_post,
        delete_tag,
        delete_metric,
        delete_doc,
    ),
    components(schemas(User, Post, Tag, Metric, Doc)),
    tags(
        (
            name = stringify!(User),
            description = concat!(stringify!(User), " operations")
        ),
        (
            name = stringify!(Post),
            description = concat!(stringify!(Post), " operations")
        ),
        (name = stringify!(Tag), description = concat!(stringify!(Tag), " operations")),
        (
            name = stringify!(Metric),
            description = concat!(stringify!(Metric), " operations")
        ),
        (name = stringify!(Doc), description = concat!(stringify!(Doc), " operations"))
    )
)]
pub struct ApiDoc;
/// Get OpenAPI specification as JSON
pub fn openapi_json() -> String {
    ApiDoc::openapi().to_json().unwrap()
}
/// The data-plane routes (CRUD + WS subscriptions) with the database
/// state still unbound.  Factored out so the tenant-auth guard can wrap
/// ONLY these routes, leaving the operational endpoints unauthenticated
/// (Phase 5).
fn __data_routes() -> Router<Arc<RwLock<super::Database>>> {
    Router::new()
        .route(concat!("/api/", "user"), get(list_user))
        .route(concat!("/api/", "user"), post(create_user))
        .route(
            concat!("/api/", "user", "/{id}"),
            get(get_user).put(update_user).delete(delete_user),
        )
        .route(concat!("/subscribe/", "user"), get(subscribe_user))
        .route(concat!("/live-query/", "user"), get(subscribe_live_user))
        .route(concat!("/api/", "post"), get(list_post))
        .route(concat!("/api/", "post"), post(create_post))
        .route(
            concat!("/api/", "post", "/{id}"),
            get(get_post).put(update_post).delete(delete_post),
        )
        .route(concat!("/subscribe/", "post"), get(subscribe_post))
        .route(concat!("/live-query/", "post"), get(subscribe_live_post))
        .route(concat!("/api/", "tag"), get(list_tag))
        .route(concat!("/api/", "tag"), post(create_tag))
        .route(
            concat!("/api/", "tag", "/{id}"),
            get(get_tag).put(update_tag).delete(delete_tag),
        )
        .route(concat!("/subscribe/", "tag"), get(subscribe_tag))
        .route(concat!("/live-query/", "tag"), get(subscribe_live_tag))
        .route(concat!("/api/", "metric"), get(list_metric))
        .route(concat!("/api/", "metric"), post(create_metric))
        .route(
            concat!("/api/", "metric", "/{id}"),
            get(get_metric).put(update_metric).delete(delete_metric),
        )
        .route(concat!("/subscribe/", "metric"), get(subscribe_metric))
        .route(concat!("/live-query/", "metric"), get(subscribe_live_metric))
        .route(concat!("/api/", "doc"), get(list_doc))
        .route(concat!("/api/", "doc"), post(create_doc))
        .route(
            concat!("/api/", "doc", "/{id}"),
            get(get_doc).put(update_doc).delete(delete_doc),
        )
        .route(concat!("/subscribe/", "doc"), get(subscribe_doc))
        .route(concat!("/live-query/", "doc"), get(subscribe_live_doc))
        .route("/replicate", get(__replicate))
}
/// The operational routes (Phase 5): liveness / readiness / minimal
/// metrics.  Never behind the tenant-auth guard so infra probes and
/// metric scrapers reach them without a JWT.
fn __ops_routes() -> Router<Arc<RwLock<super::Database>>> {
    Router::new()
        .route("/health", get(__health))
        .route("/ready", get(__ready))
        .route("/metrics", get(__metrics))
        .route("/snapshot", get(__snapshot))
}
/// Create the API router with all endpoints (no auth).  A
/// `tower_http::trace::TraceLayer` wraps every route so each request is
/// logged as a structured `tracing` span (level via `RUST_LOG`) — the
/// server-side half of Phase 5 observability; the scaffold
/// `main.rs` installs the subscriber.
pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {
    create_router_with_options(db, HttpOptions::default())
}
/// Process-start HTTP options (#140, epic #126 Tier C).
///
/// Deployment identity, never baked at generate time: the same generated
/// binary is promoted to localhost, staging, and production with
/// different allowed origins, so baking them would make one build
/// undeployable to two environments.
#[derive(Debug, Clone, Default)]
pub struct HttpOptions {
    /// Origins allowed to call this API cross-origin.
    ///
    /// `None` — the default — emits **no** `CorsLayer` and applies **no**
    /// WebSocket origin check, which is byte-identical to the behavior
    /// before #140. `None` is not the same as `Some(vec![])`: an empty
    /// `CorsLayer` still answers preflight `OPTIONS` with 200, whereas
    /// these routes answer 405.
    pub allowed_origins: Option<Vec<String>>,
}
/// Parse a comma-separated origin list, as read from
/// `FORGEDB_CORS_ORIGINS`.
///
/// Empty or all-whitespace input is `Ok(None)` — "not configured", not an
/// error. Entries are trimmed. Returns `Err` for an entry that is not a
/// valid header value, and for `*` mixed with explicit origins: that
/// combination has two defensible readings and picking one silently is a
/// security-relevant coin flip.
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
/// The origin allow-list as the WebSocket handlers see it.
///
/// Always present in request extensions — `AllowedOrigins(None)` when
/// unconfigured — so the handlers can take it unconditionally and the
/// "unconfigured means accept" branch lives in exactly one place.
#[derive(Debug, Clone)]
pub struct AllowedOrigins(pub Option<Arc<Vec<String>>>);
impl AllowedOrigins {
    /// Whether a handshake carrying `origin` may proceed.
    ///
    /// Unconfigured accepts everything (today's behavior, preserved). An
    /// absent `Origin` header is accepted even when configured: native
    /// `/replicate` followers, CLI tools and tests send none, rejecting
    /// them would break them, and it buys nothing — an attacker who
    /// controls the client controls the header. Origin checking defends
    /// the *browser* threat model, where the browser sets the header and
    /// the page cannot forge it.
    pub fn permits(&self, origin: Option<&str>) -> bool {
        match (&self.0, origin) {
            (None, _) => true,
            (Some(_), None) => true,
            (Some(list), Some(o)) => list.iter().any(|a| a == "*" || a == o),
        }
    }
}
/// Read the `Origin` header, if the request carries a valid one.
fn __origin_of(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok())
}
/// Build the CORS layer for `origins`, or `None` when unconfigured.
///
/// Methods are exactly the set the generated router registers; there is no
/// `PATCH` route. Headers are `content-type` (JSON bodies) and
/// `authorization` (bearer tokens when auth is on).
///
/// **No `allow_credentials`.** ForgeDB auth is a bearer token in a header,
/// not a cookie, so credentials mode is unnecessary — and because nothing
/// is auto-attached by the browser, an explicit `*` does not create a CSRF
/// vector here. It also avoids tower-http's wildcard-plus-credentials
/// conflict. Anyone later adding cookie auth must revisit this.
fn __cors_layer(
    origins: &Option<Arc<Vec<String>>>,
) -> Option<tower_http::cors::CorsLayer> {
    let list = origins.as_ref()?;
    let methods = [
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::DELETE,
    ];
    let headers = [axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION];
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
/// Apply the origin-dependent layers to an otherwise-finished router.
///
/// The `Extension` is applied unconditionally so the WS handlers can take
/// it without an optional extractor; the `CorsLayer` only when configured,
/// because an empty one would change `OPTIONS` from 405 to 200 for every
/// existing deployment.
///
/// Layer order is inverted from reading order — a layer applied later
/// wraps outer — so this runs **after** the `TraceLayer`, putting CORS
/// outermost. That is load-bearing: browsers send preflight `OPTIONS`
/// without an `Authorization` header, so a CORS layer inside the tenant
/// guard would have its preflight rejected 401 and the browser would
/// report an opaque CORS failure. Outermost also means error responses
/// (401/403/422) carry the CORS headers the browser needs in order to let
/// the page read the status.
fn __apply_origin_layers(
    router: Router<Arc<RwLock<super::Database>>>,
    opts: HttpOptions,
) -> Router<Arc<RwLock<super::Database>>> {
    let origins = opts.allowed_origins.map(Arc::new);
    let cors = __cors_layer(&origins);
    let router = router.layer(axum::Extension(AllowedOrigins(origins)));
    match cors {
        Some(layer) => router.layer(layer),
        None => router,
    }
}
/// Create the API router with process-start [`HttpOptions`] (#140).
///
/// `create_router` is this with the defaults, kept as a separate function
/// with its original signature because the scaffold writes `src/main.rs`
/// **once** at `forgedb init` and never regenerates it — changing the
/// arity of the existing constructors would break every existing project
/// the next time it ran `forgedb generate`.
pub fn create_router_with_options(
    db: Arc<RwLock<super::Database>>,
    opts: HttpOptions,
) -> Router {
    let router = __data_routes()
        .merge(__ops_routes())
        .layer(tower_http::trace::TraceLayer::new_for_http());
    __apply_origin_layers(router, opts).with_state(db)
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
    create_router_with_auth_and_options(db, auth, HttpOptions::default())
}
/// The tenant-auth router with process-start [`HttpOptions`] (#140).
///
/// The CORS layer is applied **outside** the tenant guard — see
/// `__apply_origin_layers` for why that placement is load-bearing rather
/// than incidental.
pub fn create_router_with_auth_and_options(
    db: Arc<RwLock<super::Database>>,
    auth: Arc<forgedb_auth::Authenticator>,
    opts: HttpOptions,
) -> Router {
    let guarded = __data_routes()
        .layer(
            axum::middleware::from_fn_with_state(
                auth,
                forgedb_auth::axum_mw::require_tenant,
            ),
        );
    let router = guarded
        .merge(__ops_routes())
        .layer(tower_http::trace::TraceLayer::new_for_http());
    __apply_origin_layers(router, opts).with_state(db)
}
