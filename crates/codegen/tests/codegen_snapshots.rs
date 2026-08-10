//! Snapshot tests for code generation
//!
//! Uses insta for snapshot testing to ensure generated code remains stable.

use forgedb_codegen::{
    ApiGenerator, FfiGenerator, GenConfig, GoGenerator, GoSdkGenerator, HopPlan, ModelOp,
    NapiGenerator, OpenApiGenerator, PyO3Generator, PythonSdkGenerator, RustGenerator,
    RustSdkGenerator, TransformGenerator, TransformPlan, TypeScriptGenerator, VersionSchema,
    WasmGenerator,
};
use forgedb_codegen::{EngineHopPlan, EngineMigrationGenerator};
use forgedb_parser::ast::{ComponentProtocol, ComponentReference, IndexType, RelationInclusion};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema, TimestampPrecision};

/// Helper to create a simple test schema with one model
fn simple_user_schema() -> Schema {
    Schema {
        models: vec![Model { position: None,
            name: "User".to_string(),
            fields: vec![
                Field { position: None,
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
                Field { position: None,
                    name: "email".to_string(),
                    field_type: FieldType::String,
                    auto_generate: false,
                    unique: true,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "age".to_string(),
                    field_type: FieldType::OptionalStructType("u32".to_string()),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    }
}

/// Helper to create a schema with multiple models
fn multi_model_schema() -> Schema {
    Schema {
        models: vec![
            Model { position: None,
                name: "User".to_string(),
                fields: vec![Field { position: None,
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
                }],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            },
            Model { position: None,
                name: "Post".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
                        name: "title".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            },
        ],
        structs: vec![],
        enums: vec![],
    }
}

#[test]
fn test_rust_generation_simple_model() {
    let schema = simple_user_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_rust_generation_multiple_models() {
    let schema = multi_model_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

/// Guard for the `+` auto-generate synthesis + create contract (#187/#188).
/// A model with `+uuid` and `+timestamp` auto fields must:
///   * carry serde defaults so a create body may omit them (uuid → `#[serde(default)]`,
///     timestamp → `#[serde(default = "__forgedb_default_ts")]`);
///   * emit the `__forgedb_default_ts` helper;
///   * synthesize omitted values in `create_<model>` (`Uuid::new_v4()` for a nil
///     uuid, `Timestamp::now()` for a zero timestamp), over a `mut record`.
#[test]
fn test_rust_generation_auto_generate_synthesis() {
    fn auto_field(name: &str, ft: FieldType) -> Field {
        Field {
            position: None,
            name: name.to_string(),
            field_type: ft,
            auto_generate: true,
            unique: false,
            indexed: false,
            constraints: vec![],
            index_type: IndexType::Hash,
            is_computed: false,
            fulltext_indexed: false,
            is_materialized: false,
        }
    }
    let schema = Schema {
        models: vec![Model {
            position: None,
            name: "Event".to_string(),
            fields: vec![
                auto_field("id", FieldType::Uuid),
                auto_field("created_at", FieldType::Timestamp(TimestampPrecision::Millis)),
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    };
    let code = RustGenerator::generate(&schema).unwrap().code;

    // serde defaults let a create body omit auto fields.
    assert!(code.contains("#[serde(default)]"), "uuid auto field needs #[serde(default)]");
    assert!(
        code.contains("#[serde(default = \"__forgedb_default_ts\")]"),
        "timestamp auto field needs the default-fn attr"
    );
    assert!(
        code.contains("fn __forgedb_default_ts() -> Timestamp"),
        "the timestamp default helper must be emitted"
    );

    // create_<model> synthesizes omitted values over a `mut record`.
    assert!(
        code.contains("pub fn create_event(&mut self, mut record: Event)"),
        "create takes `mut record` for synthesis"
    );
    assert!(code.contains("record.id = Uuid::new_v4()"), "nil uuid → new_v4");
    assert!(code.contains("record.created_at = Timestamp::now()"), "zero timestamp → now");
}

#[test]
fn test_rust_generation_has_utoipa_derives() {
    let schema = simple_user_schema();
    let result = RustGenerator::generate(&schema).unwrap();

    // Verify utoipa imports and derives are present (formatted output)
    assert!(result.code.contains("use utoipa::ToSchema"));
    assert!(result.code.contains("use serde::{Deserialize, Serialize}"));
    assert!(result.code.contains("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]"));
}

#[test]
fn test_api_generation_simple_model() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_api_generation_multiple_models() {
    let schema = multi_model_schema();
    let result = ApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_api_generation_has_utoipa_attributes() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify utoipa imports are present (formatted output)
    assert!(result.code.contains("use utoipa::OpenApi"));

    // Verify utoipa path attributes are present
    assert!(result.code.contains("#[utoipa::path"));

    // Verify OpenAPI derive is present
    assert!(result.code.contains("#[derive(OpenApi)]"));
    assert!(result.code.contains("#[openapi"));

    // Verify openapi_json function exists
    assert!(result.code.contains("pub fn openapi_json"));
}

#[test]
fn test_api_generation_has_all_crud_operations() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify all CRUD handlers are generated
    assert!(result.code.contains("async fn list_user"));
    assert!(result.code.contains("async fn get_user"));
    assert!(result.code.contains("async fn create_user"));

    // Verify router function exists
    assert!(result.code.contains("pub fn create_router"));
}

#[test]
fn test_api_generation_has_update_delete_endpoints() {
    // #69: generated REST exposes update (PUT) + delete (DELETE) over the
    // generated update()/delete() (#66), not just insert.
    let schema = simple_user_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("async fn update_user"));
    assert!(code.contains("async fn delete_user"));
    // Wired on the /{id} route alongside get.
    assert!(code.contains(".put(update_user)"));
    assert!(code.contains(".delete(delete_user)"));
    // Backed by the generated mutation surface. Create/update/delete all route
    // through the Database-level wrappers (#91 FK/field/unique; delete semantics
    // referential integrity + cascade + set_null).
    assert!(code.contains("db.update_user(key, record)"));
    assert!(code.contains("db.create_user(record)"));
    assert!(code.contains("db.delete_user(key)"));
}

#[test]
fn test_rust_generation_root_threading() {
    // #59: generated storage + Database must be openable under an arbitrary root
    // (per-tenant data dir), and the no-arg constructors stay (CWD-relative).
    let schema = multi_model_schema();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Per-storage root-aware constructor + the delegating no-arg one.
    assert!(code.contains("pub fn new_at(root: &std::path::Path)"));
    assert!(code.contains("pub fn new()"));
    // Database-wide root open threads a PathBuf.
    assert!(code.contains("pub fn open_at(root: std::path::PathBuf)"));
    // Paths are joined under root, not hardcoded CWD-relative literals.
    assert!(code.contains("root.join("));
    assert!(
        !code.contains("PathBuf::from(\""),
        "no hardcoded CWD-relative column paths should remain"
    );
    // write_manifest is root-scoped so per-tenant manifests land under the tenant dir.
    assert!(code.contains("fn write_manifest(&self, root: &std::path::Path)"));
}

#[test]
fn test_api_generation_tenant_auth_router() {
    // #59: the auth-guarded router variant layers the forgedb-auth tenant guard;
    // the unguarded create_router stays for non-tenant use.
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub fn create_router("));
    assert!(code.contains("pub fn create_router_with_auth("));
    assert!(code.contains("auth: Arc<forgedb_auth::Authenticator>"));
    assert!(code.contains("forgedb_auth::axum_mw::require_tenant"));
    assert!(code.contains("axum::middleware::from_fn_with_state"));
}

#[test]
fn test_api_generation_observability_endpoints() {
    // Phase 5: liveness/readiness/metrics handlers, the unauthenticated ops
    // routes, and the request-logging layer on the generated router.
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // Handlers.
    assert!(code.contains("async fn __health("));
    assert!(code.contains("async fn __ready("));
    assert!(code.contains("async fn __metrics("));
    // Wired routes.
    assert!(code.contains("\"/health\""));
    assert!(code.contains("\"/ready\""));
    assert!(code.contains("\"/metrics\""));
    // Structured request logging (tower-http trace span per request).
    assert!(code.contains("TraceLayer::new_for_http()"));
    // Ops routes are factored + merged AFTER the auth guard so they stay
    // unauthenticated: the guard layers only __data_routes(); __ops_routes() is
    // merged in afterwards.
    assert!(code.contains("fn __data_routes()"));
    assert!(code.contains("fn __ops_routes()"));
    // Metrics reports per-model row counts (generated by naming storage fields).
    assert!(code.contains(".row_count()"));
}

#[test]
fn test_api_generation_pagination_knobs() {
    // #141 (epic #126): the list endpoint clamps against generate-time-baked
    // page bounds (PAGE_DEFAULT_LIMIT / PAGE_MAX_LIMIT), not the substrate's fixed
    // consts. Default 50 / 1000 (byte-identical); configurable via [server].
    let schema = multi_model_schema();

    // Default config: 50 / 1000, and the handler re-derives the limit against them.
    let d = ApiGenerator::generate(&schema).unwrap().code;
    assert!(
        d.contains("const PAGE_DEFAULT_LIMIT: usize = 50"),
        "default page default limit is 50 (#141)"
    );
    assert!(
        d.contains("const PAGE_MAX_LIMIT: usize = 1000"),
        "default page max limit is 1000 (#141)"
    );
    assert!(
        d.contains(".unwrap_or(PAGE_DEFAULT_LIMIT)") && d.contains(".clamp(1, PAGE_MAX_LIMIT)"),
        "the list handler clamps the limit against the baked bounds (#141)"
    );

    // Custom config: bounds honored.
    let cfg = GenConfig {
        page_default_limit: 25,
        page_max_limit: 500,
        ..GenConfig::DEFAULT
    };
    let c = ApiGenerator::generate_with_config(&schema, cfg).unwrap().code;
    assert!(
        c.contains("const PAGE_DEFAULT_LIMIT: usize = 25"),
        "page default limit is configurable (#141)"
    );
    assert!(
        c.contains("const PAGE_MAX_LIMIT: usize = 500"),
        "page max limit is configurable (#141)"
    );
    // The OpenAPI param description reflects the baked bounds.
    assert!(
        c.contains("clamped to [1, 500]; default 25"),
        "the OpenAPI limit description reflects the baked bounds (#141)"
    );
}

#[test]
fn test_api_generation_metrics_toggle() {
    // #151 (epic #126, Tier A): [server].metrics gates emission of the /metrics
    // handler + route. Default ON (byte-identical); OFF omits both. /health,
    // /ready, /snapshot are unaffected.
    let schema = multi_model_schema();

    // Default: /metrics handler + route emitted.
    let d = ApiGenerator::generate(&schema).unwrap().code;
    assert!(d.contains("async fn __metrics("), "default emits __metrics (#151)");
    assert!(d.contains("\"/metrics\""), "default wires the /metrics route (#151)");

    // metrics = false: neither the handler nor the route is emitted; the other
    // ops endpoints remain.
    let cfg = GenConfig {
        metrics: false,
        ..GenConfig::DEFAULT
    };
    let c = ApiGenerator::generate_with_config(&schema, cfg).unwrap().code;
    assert!(!c.contains("async fn __metrics("), "metrics=false omits __metrics (#151 Tier A)");
    assert!(!c.contains("\"/metrics\""), "metrics=false omits the /metrics route (#151)");
    assert!(c.contains("async fn __snapshot("), "/snapshot is unaffected by the metrics toggle");
    assert!(c.contains("\"/health\"") && c.contains("\"/ready\""), "/health + /ready unaffected");
}

#[test]
fn test_api_generation_snapshot_reads() {
    // #85: point-in-time reads over REST. `?as_of=<watermark>` swaps the row
    // source to the generated `all_at`/`get_at` snapshot reads, `/snapshot`
    // reports the current per-model watermarks, and the wire token stays an
    // opaque `usize`. PM gate constraints: (1) the snapshot branch routes through
    // the SAME generated filter body — no parallel handler; (2) `as_of` is a
    // scalar watermark, never a wall-clock instant → non-numeric is a 400.
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // List/get read the snapshot via the already-generated `all_at`/`get_at` over
    // `forgedb_storage::Snapshot::new(<watermark>)` — the substrate constructor,
    // not a new engine surface.
    assert!(code.contains("all_at(&forgedb_storage::Snapshot::new(__w))"));
    assert!(code.contains("get_at(&forgedb_storage::Snapshot::new(__w)"));

    // Constraint 2: `as_of` parses as an opaque `usize` (same class as
    // limit/offset), and a non-numeric value is a 400 — never a silent live
    // fallback, never a timestamp lookup.
    assert!(code.contains("params.get(\"as_of\")"));
    assert!(code.contains(".parse::<usize>()"));
    assert!(code.contains("StatusCode::BAD_REQUEST"));
    assert!(code.contains("as_of must be a non-negative integer watermark"));

    // Constraint 1: no parallel snapshot handler — `page`/`total` are produced by
    // a single `match __as_of` whose `as_of` arm reads `all_at(&Snapshot)` and
    // feeds the SAME closed-set `<model>_event_matches` filter.  (#160: the live
    // arm uses the narrow `__<model>_scan_matches`, generated from the SAME
    // per-field checks — one predicate source, no drift.)
    assert!(code.contains("match __as_of"));
    assert!(code.contains("_event_matches"));

    // `/snapshot` token: a schema-wide handler + route reporting the per-model
    // watermarks as a fixed per-schema key set (row-count values), the read-side
    // peer of `/metrics` — no dynamic `match model_name` dispatch anywhere.
    assert!(code.contains("async fn __snapshot("));
    assert!(code.contains("\"/snapshot\""));
    assert!(code.contains("\"watermarks\""));
    assert!(!code.contains("match model_name"));
}

#[test]
fn test_rust_generation_list_scan_narrow() {
    // #160 (A): the live list path filters/sorts a narrow scan record (id +
    // filterable/sortable columns) and full-materializes ONLY the paginated page,
    // instead of decoding every column of every row via `all()`.  The narrow
    // filter/sort reuse the SAME per-field checks/arms as `_event_matches` /
    // `_apply_sort` (one predicate source, no drift).
    let src = r#"
User {
  id: +uuid
  email: &string
  status: ^string
  region: string
  age: u32
  @index(status, region)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let db_code = RustGenerator::generate(&schema).unwrap().code;
    let api_code = ApiGenerator::generate(&schema).unwrap().code;

    // database.rs: an internal narrow scan view + a scope to read it in, NOT a wire
    // type (no Serialize/ToSchema derive).  #228 deleted the OWNED scan record and
    // its per-row decoder: nothing the scan decodes leaves the scope any more, so
    // there is nothing to own.
    assert!(db_code.contains("pub struct UserScanRef<'a>"), "#160/#224: narrow scan view emitted");
    assert!(!db_code.contains("UserScanRow"),
        "#228: the owned scan record is gone — the scope is the only scan surface");
    assert!(!db_code.contains("fn __scan_row_at("),
        "#228: the per-row narrow decoder is gone with its only caller");
    assert!(!db_code.contains("to_owned_row"),
        "#228: nothing materializes a scan row");
    // The scan view carries filterable columns but NOT the model's derives.
    let scan_struct = &db_code[db_code.find("pub struct UserScanRef<'a>").unwrap()..];
    let scan_struct = &scan_struct[..scan_struct.find('}').unwrap()];
    assert!(scan_struct.contains("status") && scan_struct.contains("age"),
        "#160: scan view carries filterable fields");

    // api.rs: narrow filter/sort helpers, and the live list uses them + materializes
    // only the page ids (never `all()` on the live path).
    assert!(api_code.contains("fn __user_scan_matches("), "#160: narrow filter helper");
    assert!(api_code.contains("fn __user_scan_sort("), "#160: narrow sort helper");
    assert!(!api_code.contains("__user_scan_matches_ref"),
        "#228: one scan filter, not an owned/borrowed pair — the owned operand is gone");
    // prettyplease wraps the scan call and its callback across lines; collapse
    // whitespace so the assertions track the shape, not the formatting.
    let api_flat: String = api_code.split_whitespace().collect::<Vec<_>>().join(" ");
    // #224 moved the filter INTO the scan, so a rejected row never allocates its
    // strings; #228 moved the sort/count/page in too, so a SURVIVING row does not
    // either.  The #160 guarantee is unchanged: the live list still sources from the
    // narrow scan, never `all()`.
    assert!(
        api_flat.contains("db .user .__with_scan("),
        "#160/#224/#228: live list scans narrowly, inside the scan scope.\nGot: {api_flat}"
    );
    assert!(api_code.contains("__user_scan_matches(r, &params)"), "#160: live list filters narrow");
    assert!(api_code.contains(".filter_map(|__id| db.user.get(*__id))"),
        "#160: only the paginated page is full-materialized");
    // The as_of branch keeps the full-record path (unchanged correctness).
    assert!(api_code.contains("all_at(&forgedb_storage::Snapshot::new(__w))"),
        "#160: as_of retains the full snapshot read");
    // #228: sort, count and pagination all happen INSIDE the callback, and only
    // `(total, ids)` comes back out.  This is the constraint the issue commits to —
    // if a future list feature needs more than ids from a scan, it comes inside.
    assert!(
        api_flat.contains(
            "|__scan: &mut Vec<super::UserScanRef<'_>>| { __user_scan_sort(__scan, &qp.sort); \
             let __total = __scan.len(); let __ids: Vec<_> = qp .pagination .apply(__scan) \
             .iter() .map(|r| r.id) .collect(); (__total, __ids) }"
        ),
        "#228: filter/sort/count/page run inside the scope; only (total, ids) escape.\nGot: {api_flat}"
    );

    // #160 (C): index pushdown — an eligible indexed field's list filter resolves
    // candidate ROWS from that field's index instead of scanning every row.  #228
    // reduced this to row resolution: the decode is the scan scope's, so the
    // pushdown arm now gets the borrowed view it could never have while it read its
    // candidates positionally.
    assert!(db_code.contains("pub fn __rows_by_status(&self, value: &str) -> Option<Vec<usize>>"),
        "#160 C/#228: indexed field resolves candidate rows");
    assert!(db_code.contains("pub fn __rows_by_email(&self, value: &str) -> Option<Vec<usize>>"),
        "#160 C/#228: unique-indexed field resolves candidate rows");
    assert!(api_code.contains("db.user.__rows_by_status(__v)"),
        "#160 C: live list tries index pushdown");
    assert!(
        api_flat.contains("} else { None }; let (total, __page_ids) = db .user .__with_scan( __sel,"),
        "#160 C: a parse-failure falls back to the full scan (never misses a match).\nGot: {api_flat}"
    );
    // `region` is only in a COMPOSITE index (no single-field index), so it is NOT a
    // pushdown field — it falls through to the narrow scan.
    assert!(!db_code.contains("fn __rows_by_region("),
        "#160 C: a composite-only field is not a single-field pushdown");

    // #168: the scan bulk-loads each scan column once and decodes from memory
    // (physical row order + `gather_buffered`) instead of a per-row read syscall
    // storm.  A churn-free selection is the dense prefix, so `export` aliases the
    // column via mmap; deleted rows are excluded by one bulk tombstone read.
    let scan = &db_code[db_code.find("pub fn __with_scan<R>").unwrap()..];
    let scan = &scan[..scan.find("fn __rows_by_").unwrap_or(scan.len())];
    let scan_flat: String = scan.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(scan.contains("struct __UserScanBufs"),
        "#168: local buffered-column holder emitted");
    assert!(scan.contains(".gather_buffered(&__rows)"),
        "#168: each scan column is bulk-loaded once");
    assert!(scan.contains("forgedb_storage::BufferedFixedColumn"),
        "#168: fixed scan columns use the buffered fixed reader");
    assert!(scan.contains("forgedb_storage::BufferedVariableColumn"),
        "#168: string scan columns use the buffered variable reader");
    assert!(scan_flat.contains("self.tombstones .live_indices(&__all)"),
        "#168: deleted rows excluded by one bulk tombstone read.\nGot: {scan_flat}");
    assert!(scan.contains("__all.sort_unstable()"),
        "#168: live rows iterated in physical (ascending) order");
    // #228: an index-pushdown selection is sorted too — `gather_buffered` bounds its
    // reads to [min, max], so ascending order keeps the spanned read tight.  It is
    // NOT tombstone-filtered: delete removes the id from every secondary index, so a
    // candidate resolved through one is live by construction.
    assert!(scan_flat.contains("Some(mut __c) => { __c.sort_unstable(); __c }"),
        "#228: a pushdown selection is sorted for span locality.\nGot: {scan_flat}");
    // The buffered loop decodes by SLOT via the reused field_read_stmt bodies —
    // never a per-row positional read against `self.<col>` inside the scan loop.
    assert!(scan.contains("for __slot in 0..__n"),
        "#168: buffered decode iterates slots");
    assert!(scan.contains("f(&mut __refs)"),
        "#228: the scope hands the borrowed views to the caller's callback");
}

#[test]
fn test_rust_generation_ordered_index() {
    // #169: ordered-eligible indexed fields get a PARALLEL BTreeMap index keyed by
    // the typed value + a `find_by_<field>_range` query, alongside the untouched
    // hash index. Ineligible indexed fields (string, nullable, f64) do NOT.
    let src = r#"
Metric {
  id: +uuid
  name: ^string
  views: ^u64
  score: ^i64?
  ratio: ^f64
  price: ^decimal
  at: ^timestamp
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let db_code = RustGenerator::generate(&schema).unwrap().code;

    // Eligible: u64 / i64(non-null) / decimal / timestamp get an ordered map +
    // range method, keyed by the typed value.
    assert!(db_code.contains("views_ordered"), "#169: u64 gets an ordered index");
    assert!(db_code.contains("BTreeMap<u64, std :: collections :: BTreeSet")
            || db_code.contains("BTreeMap<u64,"),
        "#169: ordered index keyed by the typed value (u64), not a String");
    assert!(db_code.contains("pub fn find_by_views_range"), "#169: u64 range/top-N method");
    assert!(db_code.contains("pub fn find_by_price_range"), "#169: decimal range method");
    assert!(db_code.contains("pub fn find_by_at_range"), "#169: timestamp range method");
    assert!(db_code.contains("price_ordered"), "#169: decimal ordered index");
    // Decimal bound is normalized (scale-invariant), like its hash key.
    let price_range = &db_code[db_code.find("fn find_by_price_range").unwrap()..];
    let price_range = &price_range[..price_range.find("__out\n").unwrap_or(price_range.len().min(1200))];
    assert!(price_range.contains("normalize"),
        "#169: decimal range bounds normalized to match the stored key");

    // The parallel hash index is untouched (exact-match path preserved).
    assert!(db_code.contains("views_index"), "#169: hash index kept alongside (parallel, not replace)");
    assert!(db_code.contains("pub fn find_by_views"), "#169: exact-match probe still emitted");

    // f64 IS ordered (#242), but uniquely: the map is keyed by the encoded u64
    // while the caller still passes an f64. Both halves are asserted, because
    // getting only the first right would compile and be unusable.
    assert!(db_code.contains("ratio_ordered"), "#242: f64 gets an ordered index");
    assert!(db_code.contains("pub fn find_by_ratio_range"), "#242: f64 range method");
    let ratio_range = &db_code[db_code.find("fn find_by_ratio_range").unwrap()..];
    let ratio_range = &ratio_range[..ratio_range.find("__out\n").unwrap_or(ratio_range.len().min(1200))];
    assert!(
        ratio_range.contains("min : Option < f64 >") || ratio_range.contains("min: Option<f64>"),
        "#242: the caller passes an f64 bound, never the encoded u64: {ratio_range}"
    );
    assert!(
        ratio_range.contains("__forgedb_f64_key"),
        "#242: the bound is encoded on the way in, so it is comparable to the stored key"
    );

    // Ineligible: string / nullable-i64 get NO ordered index or range method.
    assert!(!db_code.contains("name_ordered"), "#169: string is exact-match only");
    assert!(!db_code.contains("find_by_name_range"), "#169: no range on a string index");
    assert!(!db_code.contains("score_ordered"), "#169: nullable ordered field deferred");
    assert!(!db_code.contains("find_by_score_range"), "#169: no range on a nullable field");
}

#[test]
fn test_api_openapi_doc_structure() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify OpenAPI doc struct has correct structure (formatted output)
    assert!(result.code.contains("pub struct ApiDoc"));
    assert!(result.code.contains("paths("));
    assert!(result.code.contains("components("));
    assert!(result.code.contains("schemas("));
    assert!(result.code.contains("tags("));
}

#[test]
fn test_different_field_types() {
    let schema = Schema {
        models: vec![Model { position: None,
            name: "ComplexModel".to_string(),
            fields: vec![
                Field { position: None,
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
                Field { position: None,
                    name: "count".to_string(),
                    field_type: FieldType::I64,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "price".to_string(),
                    field_type: FieldType::F64,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "active".to_string(),
                    field_type: FieldType::Bool,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "created_at".to_string(),
                    field_type: FieldType::Timestamp(TimestampPrecision::Millis),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    };

    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

/// Helper to create a schema with complex fixed-size types
fn complex_types_schema() -> Schema {
    use forgedb_parser::Struct;
    
    Schema {
        structs: vec![
            Struct { position: None,
                name: "Address".to_string(),
                fields: vec![
                    Field { position: None,
                        name: "street".to_string(),
                        field_type: FieldType::Bytes(100),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field { position: None,
                        name: "city".to_string(),
                        field_type: FieldType::Bytes(50),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
            },
            Struct { position: None,
                name: "Location".to_string(),
                fields: vec![
                    Field { position: None,
                        name: "lat".to_string(),
                        field_type: FieldType::F64,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::BTree,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field { position: None,
                        name: "lon".to_string(),
                        field_type: FieldType::F64,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::BTree,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
            },
        ],
        enums: vec![],
        models: vec![Model { position: None,
            name: "Place".to_string(),
            fields: vec![
                Field { position: None,
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
                Field { position: None,
                    name: "name".to_string(),
                    field_type: FieldType::Bytes(200),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "address".to_string(),
                    field_type: FieldType::StructType("Address".to_string()),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "location".to_string(),
                    field_type: FieldType::OptionalStructType("Location".to_string()),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "tags".to_string(),
                    field_type: FieldType::FixedArray(Box::new(FieldType::Bytes(20)), 5),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "scores".to_string(),
                    field_type: FieldType::FixedArray(Box::new(FieldType::F64), 10),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
    }
}

#[test]
fn test_rust_generation_with_complex_types() {
    let schema = complex_types_schema();
    let result = RustGenerator::generate(&schema);

    assert!(result.is_ok());
    let code = result.unwrap().code;

    // Print for manual inspection FIRST
    println!("Generated code:\n{}", code);

    // Verify struct definitions are generated correctly
    // Note: prettyplease adds 'usize' suffix to array sizes
    assert!(code.contains("pub name: [u8; 200usize]"), "Missing: pub name: [u8; 200usize]");
    assert!(code.contains("pub address: Address"), "Missing: pub address: Address");
    assert!(code.contains("pub location: Option<Location>"), "Missing: pub location: Option<Location>");
    assert!(code.contains("pub tags: [[u8; 20usize]; 5usize]"), "Missing: pub tags: [[u8; 20usize]; 5usize]");
    assert!(code.contains("pub scores: [f64; 10usize]"), "Missing: pub scores: [f64; 10usize]");

    // Verify storage columns are created for all fixed-size types
    assert!(code.contains("name_col"), "Missing: name_col");
    assert!(code.contains("address_col"), "Missing: address_col");
    assert!(code.contains("location_col"), "Missing: location_col");
    assert!(code.contains("tags_col"), "Missing: tags_col");
    assert!(code.contains("scores_col"), "Missing: scores_col");
}

// ---------------------------------------------------------------------------
// Task #45: the three codegen compilation gaps surfaced by the examples corpus.
// The examples corpus compile-harness proves the emitted Rust compiles AND
// round-trips; this snapshot guards the emitted shape.
// ---------------------------------------------------------------------------

#[test]
fn test_rust_generation_codegen_gaps() {
    // Nullable variable-length string (`string?`), an embedded `struct`, and an
    // integer (`+u64`) primary key — each previously made `database.rs` fail to
    // compile.
    let src = r#"
struct GeoPoint {
  latitude: f64
  longitude: f64
}

Place {
  id: +u64
  name: string
  description: string?
  location: GeoPoint
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // Gap 2: the embedded struct is emitted as a real #[repr(C)] type.
    assert!(code.contains("pub struct GeoPoint"), "struct definition must be emitted");
    assert!(code.contains("pub latitude: f64"), "struct fields must be emitted");

    // Gap 3: integer PK — identity type is u64 across map key + insert/get.
    assert!(
        code.contains("id_to_row: std::sync::Arc<HashMap<u64, usize>>"),
        "id map must be keyed by u64 (Arc-wrapped, #158)"
    );
    // insert now returns Result<u64, _> (#91 validation); the PK type still threads through.
    assert!(code.contains("-> Result<u64, ValidationError>"), "insert must return the u64 PK");
    assert!(code.contains("id: u64"), "get must take the u64 PK");

    // Gap 1: nullable string field renders as Option<String> and is encoded
    // with a presence tag (so None and Some(\"\") stay distinct).  The tag goes
    // down through `append_tagged` (#231) rather than a concatenated String, so
    // neither arm allocates; the stored bytes are unchanged.
    assert!(code.contains("pub description: Option<String>"), "nullable string field");
    assert!(
        code.contains("append_tagged(1u8, s)"),
        "Some must append the 0x01 tag alongside the borrowed value"
    );
    assert!(
        code.contains(r#"append_tagged(0u8, "")"#),
        "None must encode to a presence tag"
    );
    assert!(
        !code.contains(r"String::from('\u{0}')"),
        "the None arm must not allocate a tagged String"
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_relation_traversal() {
    // A required FK (`author: *User`), an optional FK (`editor: ?User`), a
    // reverse one-to-many (`User.posts: [Post]` <- `Post.author`), and a
    // bidirectional many-to-many (`Post.tags` <-> `Tag.posts`) — the four
    // traversal shapes generated as `Database` helpers + eager-load structs.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
  editor: ?User
  tags: [Tag]
}

Tag {
  id: +uuid
  label: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // Scan helper backing reverse/M2M lookups.
    assert!(code.contains("pub fn all(&self) -> Vec<Post>"), "scan helper");

    // A. Forward FK getters (required is a direct get; optional threads through).
    assert!(
        code.contains("pub fn post_author(&self, record: &Post) -> Option<User>"),
        "forward required FK getter"
    );
    assert!(
        code.contains("record.editor.and_then(|fk| self.user.get(fk))"),
        "forward optional FK getter uses and_then"
    );

    // B. Reverse one-to-many collection getters. Post has two FKs back to User
    // (`author` + `editor`), so the single `User.posts` collection disambiguates
    // by child field rather than emitting two same-named methods.
    assert!(
        code.contains("pub fn user_posts_by_author(&self, id: Uuid) -> Vec<Post>"),
        "reverse getter disambiguated by required FK"
    );
    assert!(
        code.contains("pub fn user_posts_by_editor(&self, id: Uuid) -> Vec<Post>"),
        "reverse getter disambiguated by optional FK"
    );
    // #100: the reverse getters now PROBE the child's FK index (O(1)) instead of
    // scanning `all()` and filtering. Required FK passes the id; optional FK wraps
    // it in `Some(_)` (the FK column is `Option<Uuid>`).
    assert!(
        code.contains("self.post.find_by_author(id)"),
        "required-FK reverse getter probes find_by_author (not a scan)"
    );
    assert!(
        code.contains("self.post.find_by_editor(Some(id))"),
        "optional-FK reverse getter probes find_by_editor(Some(id))"
    );
    assert!(
        !code.contains("record.author == id"),
        "reverse getter no longer scans on record.author == id (#100)"
    );

    // C. Many-to-many: a persisted junction struct + link/query helpers.
    assert!(code.contains("pub struct PostTagLink"), "M2M junction struct");
    assert!(
        code.contains("pub fn link_post_tag(&mut self, left: Uuid, right: Uuid)"),
        "M2M link helper (fixed left/right params)"
    );
    assert!(code.contains("pub fn post_tags(&self, id: Uuid) -> Vec<Tag>"), "M2M forward query");
    assert!(code.contains("pub fn tag_posts(&self, id: Uuid) -> Vec<Post>"), "M2M reverse query");
    assert!(
        code.contains("pub post_tag_link: PostTagLink"),
        "junction is a Database field"
    );

    // #154: the junction holds in-memory traversal indexes and the M2M getters
    // PROBE them (O(degree)) instead of scanning every link row via `pairs()`.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("left_index:std::collections::HashMap<Uuid,Vec<Uuid>>")
            && flat.contains("right_index:std::collections::HashMap<Uuid,Vec<Uuid>>"),
        "junction holds left_index/right_index traversal maps (#154)"
    );
    assert!(
        flat.contains("self.post_tag_link.rights_of(id)"),
        "forward M2M getter probes rights_of (not pairs().filter())"
    );
    assert!(
        flat.contains("self.post_tag_link.lefts_of(id)"),
        "reverse M2M getter probes lefts_of (not pairs().filter())"
    );
    assert!(
        !flat.contains(".pairs().into_iter().filter(|(left,_)|*left==id)"),
        "the live forward getter no longer scans pairs() (#154)"
    );

    // Eager-load struct bundling a record with its resolved forward refs.
    assert!(code.contains("pub struct PostWithRelations"), "eager-load struct");
    assert!(
        code.contains("pub fn post_with_relations(&self, id: Uuid) -> Option<PostWithRelations>"),
        "eager-load getter"
    );

    insta::assert_snapshot!(code);
}

// ---------------------------------------------------------------------------
// T1: Tests that exercise FK/relation and component fields (C1 regression guard)
// ---------------------------------------------------------------------------

/// Schema with a FK (RequiredReference), an optional FK (OptionalReference),
/// and a OneToMany relation — all paths that previously caused C1.
fn fk_schema() -> Schema {
    Schema {
        models: vec![
            Model { position: None,
                name: "Author".to_string(),
                fields: vec![Field { position: None,
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
                }],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            },
            Model { position: None,
                name: "Post".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
                        name: "title".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    // Required FK reference (no storage column, must default)
                    Field { position: None,
                        name: "author_id".to_string(),
                        field_type: FieldType::Relation(RelationType::RequiredReference(
                            "Author".to_string(),
                        )),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    // Optional FK reference
                    Field { position: None,
                        name: "editor_id".to_string(),
                        field_type: FieldType::Relation(RelationType::OptionalReference(
                            "Author".to_string(),
                        )),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            },
        ],
        structs: vec![],
        enums: vec![],
    }
}

/// Schema where a model has a OneToMany virtual relation and a Component field.
fn component_schema() -> Schema {
    Schema {
        models: vec![Model { position: None,
            name: "Product".to_string(),
            fields: vec![
                Field { position: None,
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
                Field { position: None,
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                // OneToMany virtual relation
                Field { position: None,
                    name: "reviews".to_string(),
                    field_type: FieldType::Relation(RelationType::OneToMany(
                        "Review".to_string(),
                    )),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                // Component reference
                Field { position: None,
                    name: "card".to_string(),
                    field_type: FieldType::Component(ComponentReference {
                        protocol: ComponentProtocol::Tsx,
                        path: "components/product/card".to_string(),
                        relations: RelationInclusion::None,
                    }),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    }
}

#[test]
fn test_rust_generation_with_fk_fields() {
    let schema = fk_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // FK fields appear in the struct
    assert!(code.contains("pub author_id: Uuid"), "author_id should be Uuid");
    assert!(code.contains("pub editor_id: Option<Uuid>"), "editor_id should be Option<Uuid>");

    // FK scalars are now PERSISTED — storage columns must exist (Task #25)
    assert!(code.contains("author_id_col: FixedColumn"), "author_id must have a storage column");
    assert!(code.contains("editor_id_col: FixedColumn"), "editor_id must have a storage column");

    // FK values must NOT be silently defaulted — they are read from their columns
    assert!(!code.contains("author_id: Default::default()"), "author_id must not use default (silent data loss)");
    // editor_id None must not appear as a hardcoded struct-literal value
    assert!(!code.contains("editor_id: None,"), "editor_id must not use None default (silent data loss)");

    // Storage paths namespaced per model (C2)
    assert!(code.contains("\"post/fixed/"), "Post paths not namespaced");
    assert!(code.contains("\"author/fixed/"), "Author paths not namespaced");

    // repr(C) present (C3)
    assert!(code.contains("#[repr(C)]"), "missing #[repr(C)]");

    // #100: FK scalars are now indexed (unconditionally — a reverse getter that
    // would otherwise scan always exists). Required FK → Uuid param; optional FK
    // → Option<Uuid> param (rides #102's null-distinct key).
    assert!(
        code.contains("author_id_index"),
        "required FK author_id must have a secondary index (#100)"
    );
    assert!(
        code.contains("editor_id_index"),
        "optional FK editor_id must have a secondary index (#100)"
    );
    assert!(
        code.contains("pub fn find_by_author_id(&self, value: Uuid) -> Vec<Post>"),
        "required FK gets a find_by_author_id(Uuid) probe (#100)"
    );
    assert!(
        code.contains("pub fn find_by_editor_id(&self, value: Option<Uuid>) -> Vec<Post>"),
        "optional FK gets a find_by_editor_id(Option<Uuid>) probe (#100 + #102)"
    );

    // Snapshot for future regression detection
    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_with_component_field() {
    let schema = component_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // Virtual fields have defaults, not reads (C1)
    assert!(code.contains("reviews: ()"), "OneToMany should default to ()");
    assert!(code.contains("card: Default::default()"), "Component should default");

    // Snapshot for future regression detection
    insta::assert_snapshot!(code);
}

#[test]
fn test_typescript_generation_snapshot() {
    let schema = fk_schema();
    let result = TypeScriptGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // H1: URL uses correct template literal, id interpolated + URL-encoded.
    assert!(
        code.contains("${encodeURIComponent(id)}"),
        "id interpolation missing in template literal"
    );
    // H1: the old malformed pattern "{}" should NOT appear
    assert!(!code.contains("{}`"), "old malformed URL pattern still present");
    // H3: kebab-case (for "Post" it's "post"; verify by checking for single-word models)
    assert!(
        code.contains("/api/post/${encodeURIComponent(id)}"),
        "get URL for Post should be /api/post/${{encodeURIComponent(id)}}"
    );
    assert!(
        code.contains("/api/author/${encodeURIComponent(id)}"),
        "get URL for Author should be /api/author/${{encodeURIComponent(id)}}"
    );
    // M1: Uuid maps to string
    assert!(code.contains("id: string"), "Uuid should be string");
    // full CRUD + typed error + pagination surface.
    assert!(code.contains("async updatePost("), "SDK should expose update");
    assert!(code.contains("async deletePost("), "SDK should expose delete");
    assert!(code.contains("export class ForgeDBError"), "SDK should define a typed error");
    assert!(code.contains("ListResult<Post>"), "list should return a paginated result");
    assert!(code.contains("export type PostCreate"), "SDK should expose a create-input type");

    insta::assert_snapshot!(code);
}

/// Verify H3 (kebab-case) with a multi-word model name
#[test]
fn test_typescript_kebab_case_multi_word() {
    use forgedb_parser::ast::IndexType;

    let schema = Schema {
        models: vec![Model { position: None,
            name: "UserProfile".to_string(),
            fields: vec![Field { position: None,
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
            }],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    };

    let result = TypeScriptGenerator::generate(&schema).unwrap();
    // H3: "UserProfile" should become "user-profile" not "userprofile"
    assert!(result.code.contains("/api/user-profile"), "multi-word model should use kebab-case");
    assert!(!result.code.contains("/api/userprofile"), "must not use plain lowercase");
}

#[test]
fn test_typescript_u32_u64_are_number() {
    use forgedb_parser::ast::IndexType;

    let schema = Schema {
        models: vec![Model { position: None,
            name: "Counter".to_string(),
            fields: vec![
                Field { position: None,
                    name: "count_u32".to_string(),
                    field_type: FieldType::U32,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field { position: None,
                    name: "count_u64".to_string(),
                    field_type: FieldType::U64,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }],
        structs: vec![],
        enums: vec![],
    };

    let result = TypeScriptGenerator::generate(&schema).unwrap();
    // M1: u32/u64 must map to `number`, not `any`
    assert!(result.code.contains("count_u32: number"), "U32 should be number");
    assert!(result.code.contains("count_u64: number"), "U64 should be number");
}

// ---------------------------------------------------------------------------
// #65: Reopen / rehydration regression guard.
//
// Generated `*Storage::new()` must rebuild the in-memory identity index from
// disk so a fresh process reads data written by a previous one — otherwise the
// database silently loses everything across a restart, and the next insert
// corrupts the id->row mapping. Snapshot-only coverage lets this get accepted
// away in an `insta review`; these named assertions make its removal a loud,
// intention-revealing failure. The runtime proof (insert -> reopen -> read) is
// the compile-and-run harness in the PR description, per the codegen
// compile-test discipline.
// ---------------------------------------------------------------------------
#[test]
fn test_rust_generation_reopen_rehydration() {
    // A uuid-PK model, an integer-PK model, and a bidirectional many-to-many
    // (whose junction also carries a `row_count` that must be rehydrated).
    let src = r#"
User {
  id: +uuid
  name: string
  groups: [Group]
}

Group {
  id: +uuid
  name: string
  members: [User]
}

Widget {
  id: +u64
  label: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Row-count anchor = tombstone file length (not a reset-to-zero).
    assert!(
        code.contains("let n = db.tombstones.len();"),
        "reopen must anchor row_count on the tombstone file length"
    );
    assert!(code.contains("db.row_count = n;"), "reopen must set row_count");

    // Identity index rebuilt by scanning the id column.
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);"),
        "reopen must rebuild id_to_row (via Arc::make_mut, #158)"
    );
    // uuid PK is read via read_uuid + from_bytes...
    assert!(
        code.contains("db.id_col.read_uuid(i)"),
        "uuid-PK reopen reads the id column via read_uuid"
    );
    // ...and an integer PK via the width-matched typed read (no from_bytes).
    assert!(
        code.contains("db.id_col.read_u64(i)"),
        "integer-PK reopen reads the id column via read_u64"
    );

    // The M2M junction rehydrates its row_count from the (last-appended) column.
    assert!(
        code.contains("let row_count = right_col.len();"),
        "junction reopen must rehydrate row_count from right_col length"
    );
}

#[test]
fn test_rust_generation_reopen_index_rebuild_is_narrow() {
    // Reopen rebuilds secondary indexes reading ONLY the indexed columns at each
    // id's newest physical row — never the full record via `db.get()`.  This is
    // what lets the wasm lazy-source backend keep non-indexed columns unhydrated
    // at open (and saves native the same over-read as file I/O).
    let src = r#"
User {
  id: +uuid
  email: &string
  age: ^u32
  bio: string?
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    // prettyplease wraps method chains across lines; collapse whitespace so the
    // column-read assertions match regardless of formatting.
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // The old full-record read is gone from the index rebuild loop.
    assert!(
        !code.contains("if let Some(__rec) = db.get(__id)"),
        "reopen index rebuild must NOT decode the whole record via db.get()"
    );
    // Row resolved from the already-built id_to_row, then a tombstone gate — the
    // same liveness check `read_at`/`get` apply, so only live values are indexed.
    assert!(
        code.contains("let __row = match db.id_to_row.get(&__id)"),
        "reopen index rebuild must resolve the physical row from id_to_row"
    );
    assert!(
        code.contains("if db.tombstones.is_deleted(__row)"),
        "reopen index rebuild must gate on the tombstone before indexing"
    );
    // Indexed columns ARE read at reopen (email is &unique, age is ^index)...
    assert!(
        flat.contains(".email_col .read_string(__row)"),
        "reopen must read the indexed email column at the resolved row"
    );
    assert!(
        flat.contains(".age_col .read_u32(__row)"),
        "reopen must read the indexed age column at the resolved row"
    );
    // ...but the NON-indexed `bio` column is never touched by the rebuild — the
    // partial-hydrate win: it stays lazy on the wasm backend at open.
    assert!(
        !flat.contains(".bio_col .read_string(__row)"),
        "reopen must NOT read the non-indexed bio column (partial hydrate)"
    );
}

#[test]
fn test_rust_generation_column_projection() {
    // A model with a `@projection` naming a subset of its columns (#113): the
    // generated projection read must touch ONLY PK + selected columns (never the
    // full record), giving a tight struct and — on the wasm backend — skipping
    // fault-in of the unselected columns.
    let src = r#"
User {
  id: +uuid
  email: &string
  age: ^u32
  bio: string?
  created_at: +timestamp

  @projection(card: email, age)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // Tight projection struct: PK + selected columns only, with the real types
    // (age is `u32`, not `Option`), and NOT the unselected `bio`/`created_at`.
    assert!(code.contains("pub struct UserCard"), "projection struct emitted");
    assert!(
        flat.contains("pub struct UserCard { pub id: Uuid, pub email: String, pub age: u32, }"),
        "UserCard = PK + selected only, tight types.\nGot: {flat}"
    );

    // The narrow decoder reads ONLY id + email + age columns...
    assert!(code.contains("pub fn read_card_at"), "read_card_at emitted");
    assert!(flat.contains(".id_col .read_uuid(row_index)"), "reads PK column");
    assert!(flat.contains(".email_col .read_string(row_index)"), "reads selected email column");
    assert!(flat.contains(".age_col .read_u32(row_index)"), "reads selected age column");

    // ...never the unselected columns inside the projection decoder, and never a
    // full-record `read_at`-style construct of the model. We check by isolating
    // the `read_card_at` body.
    let body_start = code.find("fn read_card_at").expect("has read_card_at");
    let body = &code[body_start..body_start + 800.min(code.len() - body_start)];
    assert!(!body.contains("bio_col"), "projection decoder must NOT read unselected bio column");
    assert!(!body.contains("created_at_col"), "projection decoder must NOT read unselected created_at column");
    assert!(body.contains("UserCard"), "projection decoder constructs the projection struct");

    // The read surface funnels through the shared subset decoder (get/all + `_at`).
    assert!(code.contains("pub fn get_card"), "get_card emitted");
    assert!(code.contains("pub fn all_card"), "all_card emitted");
    assert!(code.contains("pub fn get_card_at"), "snapshot get_card_at emitted");
    assert!(code.contains("pub fn all_card_at"), "snapshot all_card_at emitted");
}

#[test]
fn test_rust_generation_projection_rejects_relation_field() {
    // A projection naming a virtual relation field is a compile-time error
    // (#113 PM constraint 2): relations have no column and are not projectable.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]

  @projection(bad: name, posts)
}
Post {
  id: +uuid
  author: *User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let err = RustGenerator::generate(&schema).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("not projectable") && msg.contains("posts"),
        "expected a clear rejection of the relation field, got: {msg}"
    );
}

#[test]
fn test_rust_generation_json_type() {
    // A model with a `json` field and a nullable `json?` field. `json` rides the
    // same variable-column storage path as `string` but is typed
    // `serde_json::Value` and encoded/decoded as its serialized JSON bytes.
    let src = r#"
Event {
  id: +uuid
  payload: json
  meta: json?
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // Struct field types: serde_json::Value / Option<serde_json::Value>.
    assert!(
        flat.contains("pub payload : serde_json :: Value")
            || flat.contains("pub payload: serde_json::Value"),
        "payload field is serde_json::Value.\nGot: {flat}"
    );
    assert!(
        flat.contains("pub meta : Option < serde_json :: Value >")
            || flat.contains("pub meta: Option<serde_json::Value>"),
        "meta field is Option<serde_json::Value>.\nGot: {flat}"
    );

    // Storage: a `json` field gets a VariableColumn (same as string), NOT a
    // FixedColumn.
    assert!(
        flat.contains("payload_col : VariableColumn") || flat.contains("payload_col: VariableColumn"),
        "payload uses a VariableColumn.\nGot: {flat}"
    );

    // Write path serializes via serde_json::to_string and appends as a string.
    assert!(
        flat.contains("serde_json :: to_string") || flat.contains("serde_json::to_string"),
        "json write path serializes via serde_json::to_string.\nGot: {flat}"
    );

    // Read path decodes via serde_json::from_str over read_string.
    assert!(
        flat.contains("serde_json :: from_str") || flat.contains("serde_json::from_str"),
        "json read path decodes via serde_json::from_str.\nGot: {flat}"
    );
    assert!(
        flat.contains("payload_col .read_string") || flat.contains("payload_col.read_string"),
        "json read path reads the variable string column.\nGot: {flat}"
    );

    // Nullable json uses the same 1-byte presence tag scheme as nullable string
    // (0x01 = Some, 0x00 = None) so None vs Some(Value::Null) stay distinct —
    // written through `append_tagged` (#231), which lays down the same bytes
    // without building the concatenation.
    assert!(
        code.contains("append_tagged(1u8, &s)") && code.contains(r#"append_tagged(0u8, "")"#),
        "nullable json uses the presence-tag scheme"
    );
    // The read path still decodes the tag, so both halves stay in sync.
    assert!(
        code.contains("'\\u{1}'") || code.contains("as_bytes"),
        "nullable json read path must still split the presence tag"
    );
}

#[test]
fn test_rust_generation_layout_manifest() {
    // A uuid-PK model with a variable (string) column plus an M2M junction —
    // exercises both the model manifest and the junction manifest (#57).
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Every storage `new_at(root)` refreshes the physical-layout manifest on
    // open, rooted under the (per-tenant) data dir (#59).
    assert!(
        code.contains("db.write_manifest(root);"),
        "model new_at() must refresh the root-scoped layout manifest on open"
    );
    assert!(
        code.contains("fn write_manifest(&self, root: &std::path::Path)"),
        "a root-scoped write_manifest method must be generated"
    );

    // Manifest is built from the substrate types, layout-only.
    assert!(
        code.contains("forgedb_storage :: Manifest") || code.contains("forgedb_storage::Manifest"),
        "manifest must be the substrate Manifest, not a bespoke type"
    );
    assert!(
        code.contains("forgedb_storage :: ColumnKind :: Variable")
            || code.contains("forgedb_storage::ColumnKind::Variable"),
        "a variable (string) column must be described as ColumnKind::Variable"
    );
    assert!(
        code.contains("\"variable/string_data_1.bin\""),
        "the manifest column path must match the generated storage path exactly"
    );

    // Model row-count anchor = tombstones.bin, 1 byte/row.
    assert!(
        code.contains("\"tombstones.bin\"") && code.contains("bytes_per_row : 1usize")
            || code.contains("bytes_per_row: 1usize"),
        "model manifest must anchor row count on tombstones.bin at 1 byte/row"
    );
    // The manifest path is bound to `__manifest_abs = root.join("post/...")`
    // (so both the epoch-preserving load and the save use the one path).
    assert!(
        code.contains("root.join(\"post/manifest.json\")")
            || code.contains("root.join (\"post/manifest.json\")"),
        "model manifest path must be the root-joined model directory"
    );
    assert!(
        code.contains("save_to(&__manifest_abs)") || code.contains("save_to (& __manifest_abs)"),
        "model manifest must be written via the bound __manifest_abs path"
    );

    // Junction manifest anchors on fixed/right.bin at 16 bytes/row.
    assert!(
        code.contains("\"fixed/right.bin\"") && code.contains("bytes_per_row : 16usize")
            || code.contains("bytes_per_row: 16usize"),
        "junction manifest must anchor row count on fixed/right.bin at 16 bytes/row"
    );
    assert!(
        code.contains("post_tag_link/manifest.json") || code.contains("tag_post_link/manifest.json"),
        "junction manifest must be written under the junction directory"
    );
}

#[test]
fn test_rust_generation_compaction_epoch_bump() {
    // Incremental-backup chain token (#76): `compact()` must bump the
    // manifest's `compaction_epoch` (chain-validity generation counter) so a
    // byte-tail incremental against a pre-compaction base is refused; and
    // `write_manifest` must PRESERVE an existing epoch on reopen rather than
    // clobbering it back to 0 (the trap documented at generate_write_manifest).
    let src = r#"
Post {
  id: +uuid
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // write_manifest loads any existing manifest and carries its epoch forward
    // (no hardcoded `compaction_epoch : 0` overwrite on reopen).
    assert!(
        code.contains("Manifest :: load_from") || code.contains("Manifest::load_from"),
        "write_manifest must load the existing manifest to preserve its compaction_epoch"
    );
    assert!(
        code.contains("compaction_epoch : __compaction_epoch")
            || code.contains("compaction_epoch: __compaction_epoch"),
        "the written manifest must reuse the preserved epoch, not a hardcoded 0"
    );
    assert!(
        !code.contains("compaction_epoch : 0") && !code.contains("compaction_epoch: 0"),
        "the epoch must never be hardcoded to 0 on reopen (that clobbers a bumped chain token)"
    );

    // A generated bump_compaction_epoch increments + rewrites the manifest.
    assert!(
        code.contains("fn bump_compaction_epoch"),
        "a bump_compaction_epoch method must be generated"
    );
    assert!(
        code.contains("saturating_add (1)") || code.contains("saturating_add(1)"),
        "the epoch bump must increment by 1"
    );

    // compact() must call the bump AFTER the reopen (so the reopen-written
    // manifest, which preserves the old epoch, is then advanced).
    assert!(
        code.contains("self . bump_compaction_epoch") || code.contains("self.bump_compaction_epoch"),
        "compact() must bump the compaction epoch to break the incremental chain"
    );
    let compact_pos = code
        .find("pub fn compact")
        .expect("a compact() method must be generated");
    let bump_pos = code[compact_pos..]
        .find("bump_compaction_epoch")
        .map(|p| p + compact_pos)
        .expect("compact() must reference bump_compaction_epoch");
    let reopen_pos = code[compact_pos..]
        .find("Self :: new_at")
        .or_else(|| code[compact_pos..].find("Self::new_at"))
        .map(|p| p + compact_pos)
        .expect("compact() must reopen via new_at");
    assert!(
        bump_pos > reopen_pos,
        "the epoch bump must run AFTER the reopen (which rewrites the manifest preserving the old epoch)"
    );
}

#[test]
fn test_rust_generation_snapshot_reads() {
    // Watermark snapshot reads (#56, Direction A): every model storage gains a
    // shared `read_at` + `snapshot`/`get_at`/`all_at`/`row_count`; junctions gain
    // `pairs_at`; the Database gains a `DatabaseSnapshot` bundle + `snapshot()`,
    // and one M2M forward query (`post_tags_at`) is generated snapshot-scoped to
    // prove cross-table consistency (junction watermark AND target watermark).
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Shared read path: `get` and the snapshot accessors funnel through `read_at`.
    assert!(
        code.contains("fn read_at(&self, row_index: usize) -> Option<Post>"),
        "a shared read_at(row_index) accessor must be generated"
    );
    assert!(
        code.contains("let row_index = *self.id_to_row.get(&id)?;")
            && code.contains("self.read_at(row_index)"),
        "get must resolve the id then delegate to read_at"
    );

    // Per-model snapshot surface.
    assert!(
        code.contains("pub fn row_count(&self) -> usize"),
        "row_count (the watermark) must be exposed"
    );
    assert!(
        code.contains("pub fn snapshot(&self) -> forgedb_storage::Snapshot")
            || code.contains("pub fn snapshot(&self) -> forgedb_storage :: Snapshot"),
        "each storage must capture a substrate Snapshot"
    );
    assert!(
        code.contains("pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Post>")
            || code.contains("pub fn get_at(&self, snap: &forgedb_storage :: Snapshot, id: Uuid) -> Option<Post>"),
        "get_at must be snapshot-scoped and clamp visibility"
    );
    // #66 mutation surface: for id-bearing models `get_at`/`all_at` resolve the
    // newest version *within the watermark* per id (not a plain prefix scan), so a
    // snapshot captured before a later update/delete still sees the version live
    // as-of capture.  #159: resolution is a binary search over the id's ascending
    // version list (`id_versions`), not an O(watermark) id-column scan.
    assert!(
        code.contains("let watermark = snap.watermark();"),
        "snapshot accessors bind the watermark once"
    );
    assert!(
        code.contains("let versions = self.id_versions.get(&id)?;")
            && code.contains("let pos = versions.partition_point(|&r| r < watermark);"),
        "#159: get_at binary-searches id_versions for the newest version < watermark"
    );
    assert!(
        code.contains("for versions in self.id_versions.values() {"),
        "#159: all_at resolves each id via its version list (no O(watermark) scan)"
    );
    // The old O(watermark) id-column scan must be gone from the snapshot path.
    assert!(
        !code.contains("let mut newest: Option<usize> = None;"),
        "#159: the O(watermark) get_at scan is replaced by a binary search"
    );
    // The junction pairs_at resolves latest-wins over exactly the committed
    // prefix (delete semantics added a per-pair tombstone; `unlink`ed pairs are
    // excluded, but links appended after the snapshot are still not scanned).
    assert!(
        code.contains("self.pairs_prefix(snap.watermark())")
            && code.contains("let end = snap.watermark();"),
        "junction pairs_at must resolve latest-wins over the committed prefix"
    );

    // Junction snapshot surface.
    assert!(
        code.contains("pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)>")
            || code.contains("pub fn pairs_at(&self, snap: &forgedb_storage :: Snapshot) -> Vec<(Uuid, Uuid)>"),
        "junction must expose a watermark-clamped pairs_at"
    );

    // Database-wide snapshot bundle.
    assert!(code.contains("pub struct DatabaseSnapshot"), "DatabaseSnapshot bundle");
    assert!(
        code.contains("pub post: forgedb_storage::Snapshot")
            || code.contains("pub post: forgedb_storage :: Snapshot"),
        "DatabaseSnapshot carries a per-model watermark"
    );
    assert!(
        code.contains("pub post_tag_link: forgedb_storage::Snapshot")
            || code.contains("pub post_tag_link: forgedb_storage :: Snapshot"),
        "DatabaseSnapshot carries a per-junction watermark"
    );
    assert!(
        code.contains("pub fn snapshot(&self) -> DatabaseSnapshot"),
        "Database::snapshot() captures all watermarks together"
    );

    // The one snapshot-scoped traversal: clamps BOTH the junction (pairs_at) and
    // the resolved target (get_at) to the captured snapshot.
    assert!(
        code.contains("pub fn post_tags_at(&self, snap: &DatabaseSnapshot, id: Uuid) -> Vec<Tag>"),
        "snapshot-scoped M2M forward query must be generated"
    );
    assert!(
        code.contains("pairs_at(&snap.post_tag_link)"),
        "post_tags_at must clamp the junction to its watermark"
    );
    assert!(
        code.contains("self.tag.get_at(&snap.tag, right)"),
        "post_tags_at must clamp the resolved target to its watermark"
    );
}

#[test]
fn test_rust_generation_mutation_surface() {
    // Mutation surface (#66): generated superseding-version `update` / `delete`.
    // update appends a new version and repoints the id; delete appends a tombstoned
    // version; both preserve append-only (committed bytes never mutated), so backup
    // and watermark snapshots stay unchanged. Generated per model, for uuid AND
    // integer PKs.
    let src = r#"
Post {
  id: +uuid
  title: string
}

Counter {
  id: +u64
  label: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Signatures, per PK type. update now returns Result (#91 validation);
    // delete stays bool (no validation on delete).
    assert!(
        code.contains("pub fn update(&mut self, id: Uuid, record: Post) -> Result<bool, ValidationError>"),
        "uuid-PK update signature"
    );
    assert!(
        code.contains("pub fn delete(&mut self, id: Uuid) -> bool"),
        "uuid-PK delete signature"
    );
    assert!(
        code.contains("pub fn update(&mut self, id: u64, record: Counter) -> Result<bool, ValidationError>"),
        "integer-PK update signature"
    );
    assert!(
        code.contains("pub fn delete(&mut self, id: u64) -> bool"),
        "integer-PK delete signature"
    );

    // update: guard on existence, append a live version, repoint the id.
    assert!(
        code.contains("if !self.id_to_row.contains_key(&id)"),
        "update must no-op on an absent id"
    );

    // delete: materialize via get (also gives values to re-append), remember the
    // pre-delete live row, then append a TOMBSTONED version and repoint.
    assert!(
        code.contains("let record = match self.get(id)"),
        "delete resolves the current record"
    );
    assert!(
        code.contains("let deleted_row = *self")
            && code.contains("self.tombstones.append(true)"),
        "delete appends a tombstoned superseding version"
    );

    // Append-only red line: update/delete never mutate committed bytes — no
    // positional writer, only appends.
    assert!(
        !code.contains("write_all_at") && !code.contains("write_at"),
        "mutation must be append-only — no in-place positional writes"
    );
}

#[test]
fn test_rust_generation_durable_write_path() {
    // Durable write path (#89): the WAL is wired into the generated write path as
    // the crash-durability boundary. Every mutation records an OPAQUE row blob +
    // fsync (FsyncPolicy::Always) BEFORE any column append; reopen repairs a torn
    // column tail and replays the WAL; open_at holds a single-writer lock.
    let src = r#"
User {
  id: +uuid
  email: &string
  age: u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Each storage owns a WAL, opened under the data root with Always fsync.
    assert!(
        code.contains("wal: forgedb_wal::WalManager"),
        "storage holds a WAL handle"
    );
    assert!(
        code.contains("forgedb_wal::WalManager::open")
            && code.contains("forgedb_wal::FsyncPolicy::Always"),
        "WAL opened with fsync-on-commit durability"
    );

    // Identity red line: the WAL record is OPAQUE bytes via the Raw op — the crate
    // never sees a field. NO structured WalEntry::insert / WalValue field maps.
    assert!(
        code.contains("forgedb_wal::WalEntry::raw("),
        "commit uses the opaque Raw record path"
    );
    assert!(
        !code.contains("WalEntry::insert")
            && !code.contains("WalValue")
            && !code.contains("WalOperation::Insert"),
        "must NOT use the field-decoding structured WAL API (identity)"
    );

    // The WAL write is emitted in the write path (serialize record -> opaque bytes).
    assert!(
        code.contains("serde_json::to_vec(&record)") && code.contains(".write(&forgedb_wal::WalEntry::raw"),
        "mutations serialize the row and write it to the WAL before appends"
    );

    // Recovery: torn-tail truncation + WAL replay, generated per-model (no runtime
    // model_name dispatch), decoding into the concrete model type.
    assert!(
        code.contains("fn recover_from_wal(&mut self)"),
        "generated per-model recovery method"
    );
    assert!(
        code.contains("truncate_to_rows(__anchor)")
            && code.contains("self.wal") && code.contains(".replay("),
        "recovery repairs torn tail then replays the WAL"
    );
    assert!(
        code.contains("if let forgedb_wal::WalOperation::Raw { payload }"),
        "recovery decodes the opaque Raw payload (per-model, columns baked in)"
    );
    assert!(
        code.contains("db.recover_from_wal();"),
        "new_at runs recovery before rebuilding the identity index"
    );

    // Single-writer guard: open_at acquires the data-dir lock; new() stays lock-free.
    assert!(
        code.contains("forgedb_storage::DirLock::acquire(&root)"),
        "open_at acquires the single-writer lock"
    );
    assert!(
        code.contains("_lock: Option<forgedb_storage::DirLock>"),
        "Database holds the lock for its lifetime"
    );
}

#[test]
fn test_rust_generation_wal_checkpoint() {
    // WAL checkpoint (#89 step 2 — bound the WAL): a generated `checkpoint()`
    // fsyncs every column + tombstone THEN truncates the WAL (order load-bearing),
    // auto-invoked once `writes_since_checkpoint` reaches the generated interval.
    // A `Database::checkpoint()` forces it across every collection.  Pure generated
    // wiring over existing substrate (`flush()` / `truncate()`) — no new substrate.
    let src = r#"
User {
  id: +uuid
  email: &string
  age: u32
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  users: [User]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // A fixed, generated checkpoint interval (not config — same posture as the
    // fixed fsync policy) and an in-struct counter that drives it.
    assert!(
        code.contains("const WAL_CHECKPOINT_INTERVAL: u64"),
        "generated checkpoint interval constant"
    );
    assert!(
        code.contains("writes_since_checkpoint: u64"),
        "storage tracks mutations since the last checkpoint"
    );

    // The checkpoint makes columns durable BEFORE truncating the WAL (the
    // correctness ordering) and resets the counter. #153: durability is now a
    // coalesced push-to-drive on every column + tombstones, then ONE device
    // barrier — instead of an F_FULLFSYNC per column.
    assert!(
        code.contains("pub fn checkpoint(&mut self)"),
        "generated per-model checkpoint method"
    );
    assert!(
        code.contains("self.tombstones.sync_to_drive()")
            && code.contains("self.tombstones.barrier()")
            && code.contains("self.wal.truncate()"),
        "checkpoint syncs columns to drive + one barrier, then truncates the WAL (#153)"
    );
    // Coalesced: every checkpoint/commit pushes N columns to the drive but issues
    // only ONE device barrier, so sync-to-drive calls strictly outnumber barriers
    // (#153 — the whole point: 1 barrier per checkpoint, not N).
    assert!(
        code.matches(".sync_to_drive()").count() > code.matches(".barrier()").count(),
        "each checkpoint syncs many columns to drive but issues a single barrier (#153)"
    );
    // Ordering (durability): sync-to-drive → device barrier → WAL truncate. Match
    // on the distinct expect-message needles (prettyplease may wrap the chain).
    let sync_pos = code.find("Failed to sync tombstones to drive on checkpoint");
    let barrier_pos = code.find("Failed to issue checkpoint device barrier");
    let trunc_pos = code.find("Failed to truncate WAL on checkpoint");
    assert!(
        matches!((sync_pos, barrier_pos, trunc_pos), (Some(s), Some(b), Some(t)) if s < b && b < t),
        "sync-to-drive, then the barrier, then WAL truncate (durability ordering, #153)"
    );

    // Auto-invoked from the mutation path once the interval is reached.
    assert!(
        code.contains("self.writes_since_checkpoint += 1;")
            && code.contains("if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL"),
        "mutations count toward and trigger the auto-checkpoint"
    );

    // Database-wide force-checkpoint across every collection.
    assert!(
        code.contains("self.user.checkpoint();") && code.contains("self.tag.checkpoint();"),
        "Database::checkpoint() checkpoints every model collection"
    );

    // Care item (identity/observability): the manifest's last_checkpoint is set
    // truthfully to the row count, NOT hardcoded 0, and is NOT load-bearing for
    // recovery (recovery reads column lengths, not this field).
    assert!(
        code.contains("last_checkpoint: self.row_count as u64"),
        "manifest last_checkpoint reflects the durable row count (observability)"
    );
    assert!(
        !code.contains("last_checkpoint: 0"),
        "no hardcoded-0 checkpoint left in the manifest"
    );

    // Junctions (no WAL — a #89 boundary) still make their id columns durable at
    // checkpoint (coalesced push-to-drive + one barrier, #153) so a full
    // Database::checkpoint() leaves link rows as durable as model rows.
    assert!(
        code.contains("Failed to sync junction left column on checkpoint")
            && code.contains("Failed to issue junction checkpoint device barrier"),
        "junction checkpoint syncs its id columns + one barrier (no WAL to truncate)"
    );
}

#[test]
fn test_rust_generation_version_guard() {
    // Format-version guard (#74 Phase 1): the generated app, on open, compares the
    // manifest's stamped schema serial against a codegen-baked
    // `EXPECTED_SCHEMA_VERSION` and FAIL-FAST refuses a stale data dir — it never
    // reshapes/self-heals (red line DV-6).  This turns a silent byte mis-decode of
    // a dir written under an old schema into a clear refusal pointing at the
    // migration bin.
    let src = r#"
User {
  id: +uuid
  email: &string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  users: [User]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // An opaque, codegen-baked expected version constant is emitted.
    assert!(
        code.contains("const EXPECTED_SCHEMA_VERSION: u32 = 1"),
        "generated app bakes in the version it expects"
    );

    // The guard reads exactly the one opaque integer and compares it — it must NOT
    // inspect column names/types to decide anything (DV-6: refuse, don't adapt).
    assert!(
        code.contains("__m.schema_version != EXPECTED_SCHEMA_VERSION"),
        "open compares the manifest schema serial against the expected version"
    );
    assert!(
        code.contains("but this binary expects v"),
        "the mismatch panic fails fast with migration guidance"
    );

    // The guard runs in the open path, over each collection's manifest (models and
    // junctions), loading the manifest and reading only its version field.
    assert!(
        code.contains("root.join(\"user/manifest.json\")")
            && code.contains("root.join(\"tag/manifest.json\")")
            && code.contains("root.join(\"tag_user_link/manifest.json\")"),
        "the guard covers every model and junction manifest"
    );

    // Identity: the guard branch must not read column shape to self-heal — the ONLY
    // manifest field it touches in the guard is the schema serial.  (It must never
    // resolve a decoder from column names/types the way a schema engine would.)
    assert!(
        !code.contains("__m.columns") && !code.contains("m.column_type"),
        "the version guard reads no column shape (never self-heals — DV-6)"
    );

    // #74 Phase 2: the baked version is LINEAGE-SOURCED, not hardcoded — the CLI
    // threads `MigrationLineage::current_schema_version` via
    // `generate_with_schema_version`.  A schema with no lineage baselines to 1
    // (the default `generate`); a lineage at version N bakes N.
    let code_v7 = RustGenerator::generate_with_schema_version(&schema, 7)
        .unwrap()
        .code;
    assert!(
        code_v7.contains("const EXPECTED_SCHEMA_VERSION: u32 = 7"),
        "the expected version is threaded from the migration lineage, not hardcoded"
    );
}

#[test]
fn test_rust_generation_manifest_preserves_schema_version() {
    // Version writer preservation (#74 Phase 1 prerequisite): `write_manifest`
    // runs on EVERY open, so it must load any existing manifest and carry its
    // the schema serial forward (exactly as it already does for `compaction_epoch`)
    // — otherwise a reopen would clobber a migration's version bump back to the
    // baseline and silently defeat the open-time guard.  A fresh dir (no manifest)
    // is stamped with `EXPECTED_SCHEMA_VERSION`.
    let src = r#"
User {
  id: +uuid
  email: &string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // The manifest is written with a preserved-or-baseline version, NOT a hardcoded
    // constant that would clobber a bumped version on reopen.
    assert!(
        code.contains("let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)")
            && code.contains(".map(|m| m.schema_version)")
            && code.contains(".unwrap_or(EXPECTED_SCHEMA_VERSION)"),
        "write_manifest preserves an existing schema_version, baselining a fresh dir"
    );
    assert!(
        code.contains("schema_version: __schema_version"),
        "the manifest is stamped with the preserved-or-baseline version"
    );
    // The old clobbering hardcode is gone.
    assert!(
        !code.contains("schema_version: 1,"),
        "no hardcoded schema version left to clobber a bumped version on reopen"
    );
}

#[test]
fn test_rust_generation_secondary_indexes() {
    // Secondary indexes (#90): each `^index` / `&unique` scalar field gets an
    // in-memory `value-key -> {id}` map, maintained on insert/update/delete
    // (superseding-version aware) and rebuilt on reopen, plus `find_by_*` /
    // `get_by_*` probes that resolve candidates through the version-aware read
    // path — never a scan, never bypassing snapshot resolution.
    let src = r#"
User {
  id: +uuid
  email: &string
  handle: ^string
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // A per-field index structure keyed by canonical string -> set of ids,
    // Arc-wrapped for cheap reader capture (#158).
    assert!(
        code.contains("email_index: std::sync::Arc<"),
        "unique field gets a value->ids index (Arc-wrapped)"
    );
    assert!(
        code.contains("handle_index") ,
        "^index field gets an index too"
    );
    // A non-indexed field gets NO index.
    assert!(!code.contains("name_index"), "plain fields are not indexed");

    // Unique field gets an Option-returning probe; indexed field gets Vec probes.
    assert!(code.contains("pub fn get_by_email(&self, value: &str) -> Option<User>"),
        "unique -> get_by_ Option probe");
    assert!(code.contains("pub fn find_by_email(&self, value: &str) -> Vec<User>"),
        "unique also exposes find_by_ Vec probe");
    assert!(code.contains("pub fn find_by_handle(&self, value: &str) -> Vec<User>"),
        "^index -> find_by_ Vec probe");

    // The probe is an index GET (O(1) candidate lookup), not a scan/loop over all().
    assert!(
        code.contains("match self.email_index.get(&__k)"),
        "probe hits the index map, not a full scan"
    );

    // Snapshot-scoped probe exists and resolves via get_at (the snapshot's
    // version), and post-filters the resolved value against the key.
    assert!(
        code.contains("pub fn find_by_email_at(")
            && code.contains("self.get_at(snap, __id)"),
        "snapshot probe resolves candidates through the version-aware read path"
    );

    // Maintenance: insert adds, delete removes, update removes-old + adds-new.
    // (#158: the map is `Arc`-shared with readers, so mutations go through
    // `Arc::make_mut` — copy-on-write.)
    // (prettyplease may line-wrap the `.entry(..).or_default().insert(..)` chain,
    // so we assert on the `make_mut` receiver — the load-bearing CoW part.)
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.email_index)"),
        "insert/update/delete maintain the index (via Arc::make_mut)"
    );

    // Reopen rebuild is folded into the id-scan rehydrate (keyed off db.get).
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.email_index)"),
        "indexes are rebuilt from committed rows on reopen (via Arc::make_mut)"
    );
}

#[test]
fn test_rust_generation_index_followups() {
    // Phase-2 index follow-ups #100 (FK-scalar), #101 (composite @index),
    // #102 (nullable), #103 (reader probes) — all four exercised at once.
    let src = r#"
User {
  id: +uuid
  email: &string
  handle: ^string?
  region: string
  tier: u32
  posts: [Post]
  @index(region, tier)
}

Post {
  id: +uuid
  title: string
  author: *User
  reviewer: ?User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // --- #102 nullable indexing --------------------------------------------
    assert!(code.contains("handle_index"), "#102: nullable ^field is indexed");
    assert!(
        code.contains("pub fn find_by_handle(&self, value: Option<&str>) -> Vec<User>"),
        "#102: nullable string probe takes Option<&str>"
    );
    // Null-distinct key: None keys to '\u{0}', Some(\"null\") to '\u{1}null' — no collision.
    assert!(
        code.contains(r"String::from('\u{0}')"),
        "#102: None keys to a distinct null sentinel"
    );

    // --- #100 FK-scalar indexing -------------------------------------------
    assert!(code.contains("author_index"), "#100: required FK is indexed");
    assert!(code.contains("reviewer_index"), "#100: optional FK is indexed");
    assert!(
        code.contains("pub fn find_by_author(&self, value: Uuid) -> Vec<Post>"),
        "#100: required FK probe takes Uuid"
    );
    assert!(
        code.contains("pub fn find_by_reviewer(&self, value: Option<Uuid>) -> Vec<Post>"),
        "#100 + #102: optional FK probe takes Option<Uuid>"
    );
    // Reverse one-to-many getter now PROBES the FK index instead of scanning.
    assert!(
        code.contains("self.post.find_by_author(id)"),
        "#100: reverse getter user_posts probes find_by_author (not a scan)"
    );

    // --- #101 composite @index(region, tier) -------------------------------
    assert!(
        code.contains("region_tier_index"),
        "#101: composite index field named <a>_<b>_index"
    );
    assert!(
        code.contains("pub fn find_by_region_and_tier(&self, region: &str, tier: u32) -> Vec<User>"),
        "#101: composite probe find_by_<a>_and_<b> with per-component params"
    );
    // Collision-free length-prefixed composite key build.
    assert!(
        code.contains("__ck.push_str(&__p.len().to_string());") && code.contains("__ck.push(':');"),
        "#101: composite key is length-prefixed (collision-free join)"
    );

    // --- #103 reader probes (snapshot `_at` only) --------------------------
    // The reader clones each index map; the clone init only appears on the reader.
    assert!(
        code.contains("handle_index: self.handle_index.clone()"),
        "#103: reader clones the index maps"
    );
    assert!(
        code.contains("region_tier_index: self.region_tier_index.clone()"),
        "#103: reader clones the composite index too"
    );
    // The `_at` probe is emitted on BOTH writer and reader (2×); the LIVE probe
    // only on the writer (1×) — a reader has no live `get`.
    assert_eq!(
        code.matches("pub fn find_by_handle_at(").count(),
        2,
        "#103: snapshot probe emitted on writer + reader"
    );
    assert_eq!(
        code.matches("pub fn find_by_handle(").count(),
        1,
        "#103: live probe emitted on the writer only"
    );
}

#[test]
fn test_rust_generation_reader_index_maps_are_arc_shared() {
    // #158: `reader()` shares the index maps via `Arc` (O(1) capture) instead of
    // deep-cloning O(rows) per call; the writer mutates them copy-on-write via
    // `Arc::make_mut`.  Guards both the shared field type and the CoW mutation.
    let src = r#"
User {
  id: +uuid
  email: &string
  status: ^string
  region: string
  @index(status, region)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Index + composite maps and `id_to_row` are `Arc`-wrapped on BOTH the writer
    // storage and its reader handle (same field name/type), so `reader()`'s clone
    // is a refcount bump, not a data copy.  (prettyplease may line-wrap the long
    // generic, so we check the `field: std::sync::Arc<` prefix only.)
    assert!(
        code.contains("status_index: std::sync::Arc<"),
        "#158: single-field index map is Arc-wrapped"
    );
    assert!(
        code.contains("status_region_index: std::sync::Arc<"),
        "#158: composite index map is Arc-wrapped"
    );
    assert!(
        code.contains("id_to_row: std::sync::Arc<HashMap<"),
        "#158: id_to_row is Arc-wrapped"
    );
    // The reader still captures via `.clone()` (now Arc::clone, O(1)).
    assert!(
        code.contains("status_index: self.status_index.clone()"),
        "#158: reader captures the index Arc (cheap clone)"
    );

    // Writer mutations go through copy-on-write, never a direct `&mut` deref of the
    // `Arc` (which would not compile).  Add/remove/rebuild all use `Arc::make_mut`.
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.status_index)"),
        "#158: index add/remove mutates via Arc::make_mut (copy-on-write)"
    );
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.id_to_row).insert("),
        "#158: id_to_row mutates via Arc::make_mut"
    );
    // No index/id_to_row map is mutated by a bare `self.<map>.entry/insert/clear`
    // that bypasses make_mut (would fail to compile against `Arc<HashMap>`).
    for needle in [
        "self.status_index.entry(",
        "self.status_region_index.entry(",
        "self.id_to_row.insert(",
        "self.status_index.clear(",
    ] {
        assert!(
            !code.contains(needle),
            "#158: `{needle}` must route through Arc::make_mut, not a bare deref"
        );
    }
}

#[test]
fn test_rust_generation_version_index() {
    // #159: id-bearing models carry a per-id ascending version list `id_versions`
    // so snapshot reads binary-search for the newest version < watermark (O(log v))
    // instead of an O(watermark) id-column scan (which made the FK snapshot-probe
    // quadratic).  Guards the field, the write-path maintenance, and the reopen
    // rebuild.
    let src = r#"
User {
  id: +uuid
  email: &string
  status: ^string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // The version index is a per-id ascending Vec of physical rows, Arc-shared
    // (#158) so readers capture it O(1).
    assert!(
        code.contains("id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>"),
        "#159: id_versions is an Arc<HashMap<Id, Vec<row>>>"
    );
    // Write-path maintenance: every mutation appends the new row via Arc::make_mut.
    // (prettyplease wraps the `.entry(..).or_default().push(..)` chain onto separate
    // lines, so we assert on the receiver + the push arg independently.)
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.id_versions)")
            && code.contains(".push(row_index);"),
        "#159: insert/update/delete push the new version index"
    );
    // Reopen rebuild folds into the ascending id-scan (keeps each vec sorted).
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.id_versions)")
            && code.contains(".push(i);"),
        "#159: reopen rebuilds id_versions in the id-scan"
    );
    // Snapshot resolution is a binary search, not a scan.
    assert!(
        code.contains("versions.partition_point(|&r| r < watermark)"),
        "#159: get_at/all_at binary-search the version list"
    );
}

#[test]
fn test_rust_generation_data_integrity() {
    // Phase 3 (#91): enforce field constraints, &unique, and required/optional-FK
    // existence at write time; insert/update carry a ValidationError; the REST path
    // routes through Database-level create_/update_ wrappers that add FK checks.
    let src = r#"
User {
  id: +uuid
  email: &string @email
  age: u32 @min(0) @max(150)
  name: string @length(2, 50)
}

Post {
  id: +uuid
  title: string
  author: *User
  reviewer: ?User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // ValidationError type with the three integrity classes + status mapping.
    assert!(code.contains("pub enum ValidationError"), "ValidationError type emitted");
    // Every variant names its model as well as its field (#257): a field is only
    // identified by the pair, since two models may declare the same field name.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("Unique{model:&'staticstr,field:&'staticstr")
            && flat.contains("DanglingReference{model:&'staticstr,field:&'staticstr,target:&'staticstr")
            && flat.contains("Constraint{model:&'staticstr,field:&'staticstr,rule:&'staticstr,message:String"),
        "three integrity variants, each carrying (model, field)"
    );
    assert!(code.contains("pub fn status_code(&self) -> u16"), "status_code maps to HTTP");

    // Per-model field validator with the declared directives.
    assert!(code.contains("fn validate_user(record: &User) -> Result<(), ValidationError>"),
        "generated per-model field validator");
    assert!(code.contains("rule: \"email\"") && code.contains("rule: \"min\"")
        && code.contains("rule: \"max\"") && code.contains("rule: \"length\""),
        "each declared directive is enforced");

    // insert/update validate first and carry the error.
    assert!(code.contains("pub fn insert(&mut self, record: User) -> Result<Uuid, ValidationError>"),
        "insert returns Result");
    assert!(code.contains("validate_user(&record)?;"), "insert/update call the validator first");
    // &unique enforcement probes the Phase-2 unique index and rejects a duplicate.
    assert!(code.contains("self.email_index.get(&__uk)") && code.contains("ValidationError::Unique"),
        "duplicate &unique email is rejected via the unique index");

    // Database-level validated wrappers add FK existence (sibling access).
    assert!(code.contains("pub fn create_post(&mut self, mut record: Post) -> Result<Uuid, ValidationError>"),
        "create_<model> wrapper");
    assert!(code.contains("pub fn update_post(") && code.contains("-> Result<bool, ValidationError>"),
        "update_<model> wrapper");
    // Required FK checked directly; optional FK checked only when Some.
    assert!(code.contains("self.user.get(record.author).is_none()"),
        "required FK author existence checked");
    assert!(code.contains("if let Some(__fk) = record.reviewer"),
        "optional FK reviewer checked only when set");
    assert!(code.contains("ValidationError::DanglingReference"), "dangling FK rejected");
}

#[test]
fn test_rust_generation_numeric_bounds_per_domain() {
    // #239: `@min`/`@max` compare in a domain that represents both the value and
    // the bound exactly. Three defects motivated this, all in one code path:
    //
    //  1. `decimal` was absent from `is_numeric_type`, and because both arms are
    //     gated on that predicate a bound on the exact-money type emitted NO check
    //     — parsed, carried through the AST, silently enforcing nothing.
    //  2. The compare ran through `(*__v as f64)`, which `decimal` cannot even do
    //     (it is a struct, not a primitive) — so (1) could not be fixed by adding a
    //     match arm alone.
    //  3. That same f64 cast rounds a 64-bit integer past the 53-bit mantissa, so
    //     `qty: u64 @min(9007199254740993)` ACCEPTED 9007199254740992 — a value
    //     below the declared minimum.
    let src = r#"
Product {
  id: +uuid
  price: decimal @min(1) @max(1000000)
  discount: ?decimal @min(0)
  qty: u64 @min(9007199254740993)
  ratio: f64 @max(1)
  age: ?i32 @min(0)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat = code.replace([' ', '\n'], "");

    // (1) + (2): decimal bounds are enforced, compared as Decimal — never via f64.
    assert!(
        flat.contains("(*__v)<rust_decimal::Decimal::from(1i64)"),
        "decimal @min compares in the decimal domain"
    );
    assert!(
        flat.contains("(*__v)>rust_decimal::Decimal::from(1000000i64)"),
        "decimal @max compares in the decimal domain"
    );
    // A nullable decimal must get the same treatment, inside its `Some` guard.
    assert!(
        flat.contains("(*__v)<rust_decimal::Decimal::from(0i64)"),
        "nullable decimal @min is enforced too"
    );

    // (3): integer bounds promote to i128, which holds all of u64/i64 losslessly.
    assert!(
        flat.contains("(*__vasi128)<(9007199254740993i64asi128)"),
        "u64 @min compares as i128, not f64"
    );
    assert!(
        flat.contains("(*__vasi128)<(0i64asi128)"),
        "nullable i32 @min compares as i128"
    );

    // f64 fields keep the float compare — their own domain IS binary float, so
    // there is nothing more exact to promote to.
    assert!(
        flat.contains("(*__vasf64)>(1i64asf64)"),
        "f64 @max stays in the f64 domain"
    );

    // The lossy form must be gone for every non-f64 numeric field. `ratio` is the
    // only field allowed to produce an `as f64` compare.
    assert_eq!(
        flat.matches("(*__vasf64)").count(),
        1,
        "only the f64 field compares through f64"
    );

    // All four bounded fields actually emit a rule, i.e. none silently vanished.
    for field in ["price", "discount", "qty", "ratio", "age"] {
        assert!(
            code.contains(&format!("field: \"{field}\"")),
            "{field} emits a constraint check"
        );
    }
}

#[test]
fn test_rust_generation_fractional_and_exclusive_bounds() {
    // #239 gaps 1, 2 and 4: fractional bounds, exclusive bounds, and negative
    // bounds. The load-bearing assertion is the `decimal` one — the literal is
    // reconstructed as mantissa+scale, so `0.01` is the exact Decimal 1e-2 and
    // never passes through a binary float, which is the entire reason the lexeme
    // is carried from the lexer instead of being parsed to f64 there.
    let src = r#"
Product {
  id: +uuid
  price: decimal @min(0.01) @max(99999.99)
  fee: decimal @min(>0.00)
  rate: f64 @min(>0) @max(<1)
  temp: f64 @min(-273.15)
  celsius: i32 @min(-273)
  opt: ?decimal @min(0.05)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat = code.replace([' ', '\n'], "");

    // Exact decimal reconstruction: 0.01 → 1e-2, 99999.99 → 9999999e-2.
    assert!(
        flat.contains("rust_decimal::Decimal::from_i128_with_scale(1i128,2u32)"),
        "0.01 is built exactly as mantissa 1, scale 2"
    );
    assert!(
        flat.contains("rust_decimal::Decimal::from_i128_with_scale(9999999i128,2u32)"),
        "99999.99 is built exactly"
    );
    assert!(
        flat.contains("rust_decimal::Decimal::from_i128_with_scale(5i128,2u32)"),
        "a nullable decimal bound is built exactly too"
    );
    // No decimal bound may be routed through a float literal.
    assert!(
        !flat.contains("Decimal::from_str") && !flat.contains("0.01f64"),
        "a decimal bound never passes through a parse or an f64 literal"
    );

    // Exclusive bounds flip the rejecting comparison: `@min(>n)` rejects `v <= n`.
    assert!(
        flat.contains("(*__v)<=rust_decimal::Decimal::from_i128_with_scale(0i128,2u32)"),
        "@min(>0.00) rejects values <= the bound"
    );
    assert!(
        flat.contains("(*__vasf64)<=(0i64asf64)"),
        "@min(>0) is exclusive"
    );
    assert!(
        flat.contains("(*__vasf64)>=(1i64asf64)"),
        "@max(<1) is exclusive"
    );

    // A fractional f64 bound rounds only into the field's own domain.
    assert!(flat.contains("(*__vasf64)<-273.15f64"), "f64 fractional bound");

    // Negative bounds (gap 4) reach codegen at all.
    assert!(
        flat.contains("(*__vasi128)<(-273i64asi128)"),
        "negative integer bound"
    );

    // Messages quote the author's own spelling, including exclusivity and the
    // trailing zero, rather than a re-rendered float.
    assert!(code.contains(r#""must be >= 0.01""#), "inclusive message");
    assert!(code.contains(r#""must be > 0.00""#), "exclusive message");
    assert!(code.contains(r#""must be < 1""#), "exclusive max message");
    assert!(code.contains(r#""must be >= -273.15""#), "negative message");
}

#[test]
fn test_rust_generation_delete_restrict() {
    // Delete semantics — restrict (the default): deleting a parent with live
    // children is refused via ReferencedByChildren (409).  Absent @on_delete
    // defaults to restrict.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User @on_delete(restrict)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("ReferencedByChildren { model: &'static str, field: &'static str }"),
        "new ValidationError variant emitted"
    );
    assert!(
        code.contains("| ValidationError::ReferencedByChildren { .. } => 409"),
        "restrict conflict maps to 409"
    );
    assert!(
        code.contains("pub fn delete_user(&mut self, id: Uuid) -> Result<bool, ValidationError>"),
        "delete_<parent> wrapper returns Result so restrict can 409"
    );
    // The restrict child is probed via the FK index (O(1), #100) and refuses.
    assert!(
        code.contains("let __children = self.post.find_by_author(id);")
            && code.contains("if !__children.is_empty()")
            && code.contains("model: \"Post\"")
            && code.contains("field: \"author\""),
        "restrict checks the referencing children and refuses"
    );
    // A restrict child is never recursively cascade-deleted by the parent wrapper
    // (delete_user_cascade must not call the child's cascade worker).
    let user_body = code
        .split("fn delete_user_cascade")
        .nth(1)
        .and_then(|s| s.split("fn ").next())
        .unwrap_or("");
    assert!(
        !user_body.contains("delete_post_cascade"),
        "restrict parent does not cascade-delete its children"
    );
}

#[test]
fn test_rust_generation_delete_cascade() {
    // Delete semantics — cascade: deleting a parent recursively deletes children
    // (through their own wrapper so multi-level chains fire), guarded by depth.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User @on_delete(cascade)
  comments: [Comment]
}

Comment {
  id: +uuid
  body: string
  post: *Post @on_delete(cascade)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("const MAX_CASCADE_DEPTH: u32"), "cascade depth bound emitted");
    // Deleting a User cascade-deletes its Posts, recursing through the Post
    // wrapper so the Post -> Comment cascade also fires (multi-level).
    assert!(
        code.contains("let __children = self.post.find_by_author(id);")
            && code.contains("self.delete_post_cascade(__c.id, __depth + 1)?;"),
        "User cascade recurses into Post's own wrapper"
    );
    assert!(
        code.contains("let __children = self.comment.find_by_post(id);")
            && code.contains("self.delete_comment_cascade(__c.id, __depth + 1)?;"),
        "Post cascade recurses into Comment (multi-level chain)"
    );
    // Depth guard is present in the internal worker.
    assert!(
        code.contains("if __depth > MAX_CASCADE_DEPTH"),
        "cascade worker guards recursion depth (cycle safety)"
    );
}

#[test]
fn test_rust_generation_delete_set_null() {
    // Delete semantics — set_null: deleting a parent nulls each child's OPTIONAL
    // FK via the child's update path.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: ?User @on_delete(set_null)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("let __children = self.post.find_by_author(Some(id));"),
        "set_null probes the optional FK index with Some(id)"
    );
    assert!(
        code.contains("__c.author = None;") && code.contains("self.update_post(__cid, __c)?;"),
        "set_null nulls the child FK and updates it"
    );
}

#[test]
fn test_rust_generation_delete_set_null_on_required_is_codegen_error() {
    // set_null on a REQUIRED FK (`*Target`) cannot null a non-null column — this
    // is a hard codegen error, caught at generate time.
    let src = r#"
User {
  id: +uuid
  name: string
}

Post {
  id: +uuid
  author: *User @on_delete(set_null)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let result = RustGenerator::generate(&schema);
    let err = result.err().expect("set_null on a required FK must be a codegen error");
    let msg = format!("{err}");
    assert!(
        msg.contains("set_null") && msg.contains("required"),
        "error explains set_null is invalid on a required FK: {msg}"
    );
}

#[test]
fn test_rust_generation_m2m_unlink() {
    // Delete semantics — M2M unlink: junctions gain a per-pair Tombstones column;
    // `unlink_<a>_<b>` appends a retracted pair and traversal (`pairs`) filters it
    // out (latest-wins).  Cascade-deleting a linked model unlinks its junction rows.
    // Uses only the already-published Tombstones type — no substrate change.
    let src = r#"
Author {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *Author @on_delete(cascade)
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Junction carries a Tombstones column, appended in lockstep with link.
    assert!(
        code.contains("tombstones: Tombstones,"),
        "junction gains a per-pair Tombstones column"
    );
    assert!(
        code.contains("self.tombstones.append(false)"),
        "link appends a live (false) tombstone"
    );
    // unlink retracts the pair (append-only true tombstone) and Database exposes it.
    assert!(
        code.contains("pub fn unlink(&mut self, left: Uuid, right: Uuid) -> bool")
            && code.contains("self.tombstones.append(true)"),
        "unlink appends a retracted (true) pair"
    );
    assert!(
        code.contains("pub fn unlink_post_tag(&mut self, left: Uuid, right: Uuid) -> bool"),
        "Database exposes unlink_<a>_<b>"
    );
    // pairs() applies latest-wins so unlinked edges are excluded.
    assert!(
        code.contains("fn pairs_prefix(&self, end: usize) -> Vec<(Uuid, Uuid)>")
            && code.contains(".filter(|pair| !state.get(pair).copied().unwrap_or(true))"),
        "pairs resolves latest-wins, excluding retracted pairs"
    );
    // Cascade-delete of Post unlinks its junction rows on the left side.
    assert!(
        code.contains("self.post_tag_link.unlink_all_left(id);"),
        "cascade-delete unlinks the model's junction rows"
    );
}

#[test]
fn test_rust_generation_pattern_validation() {
    // #104: `@pattern("...")` / `@regex("...")` are now ENFORCED in the same
    // generated per-model `validate_<model>` as `@email`/`@length`, compiled once
    // into a `LazyLock<regex::Regex>` static and reported via the Constraint (422)
    // path.  Nullable string fields validate only when `Some`.
    let src = r#"
Account {
  id: +uuid
  code: string @pattern("^[0-9]+$")
  slug: string? @regex("^[a-z-]+$")
  age: u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // The validator exists and carries the pattern rule (Constraint → 422).
    assert!(
        flat.contains("fn validate_account(record: &Account) -> Result<(), ValidationError>"),
        "generated per-model field validator"
    );
    assert!(flat.contains("rule: \"pattern\""), "@pattern/@regex map to the Constraint rule");

    // The regex is compiled once into a LazyLock static over the plain `regex` crate,
    // and each declared pattern's source is embedded verbatim.
    assert!(
        flat.contains("std::sync::LazyLock<regex::Regex>"),
        "pattern compiled once into a LazyLock<regex::Regex> static"
    );
    assert!(flat.contains("regex::Regex::new(\"^[0-9]+$\")"), "@pattern source embedded");
    assert!(flat.contains("regex::Regex::new(\"^[a-z-]+$\")"), "@regex source embedded");
    assert!(flat.contains(".is_match(__v.as_str())"), "the value is tested with is_match");

    // The nullable `slug` field only validates its `Some` value (matches @email/@length).
    assert!(
        flat.contains("if let Some(__v) = &record.slug"),
        "nullable pattern field validated only when Some"
    );
}

#[test]
fn test_rust_generation_auto_compaction() {
    // Bounded storage (v1 Phase 4 — #92): the mutation surface (#66)
    // leaves dead row versions behind; generated code auto-compacts IN-PROCESS
    // under the single-writer lock once enough accumulate.  The reclaim itself is a
    // schema-agnostic keep-set primitive in `forgedb-compaction` — generated code
    // computes the LIVE physical-row set (the field-aware decision) and hands the
    // opaque indices over; the substrate keeps exactly those rows.
    let src = r#"
Widget {
  id: +uuid
  sku: &string
  category: ^string
  qty: u32
}

Gadget {
  id: +uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // A fixed, generated dead-version threshold (not config — same posture as the
    // WAL checkpoint interval) plus the per-storage counter + remembered root.
    assert!(
        code.contains("const COMPACTION_DEAD_THRESHOLD: u64"),
        "generated compaction threshold constant"
    );
    assert!(
        code.contains("dead_since_compaction: u64"),
        "storage tracks dead versions since the last compaction"
    );
    assert!(
        code.contains("root: std::path::PathBuf"),
        "storage remembers its data root for compact + reopen"
    );

    // The generated compact() checkpoints FIRST (no index-relative WAL tail may
    // survive the renumbering), then drives the schema-agnostic keep-set primitive.
    assert!(
        code.contains("pub fn compact(&mut self)"),
        "generated per-model compact method"
    );
    let ckpt = code.find("pub fn compact(&mut self)").and_then(|start| {
        code[start..].find("self.checkpoint();").map(|o| start + o)
    });
    let keep = code.find("compact_model_keeping");
    assert!(
        matches!((ckpt, keep), (Some(c), Some(k)) if c < k),
        "compact() checkpoints before invoking the keep-set reclaim"
    );

    // The reclaim is the keep-set primitive — NOT tombstone-based compact_model,
    // and NEVER BackgroundCompactor (which would run off the writer lock).
    assert!(
        code.contains("compact_model_keeping"),
        "generated code uses the keep-set primitive"
    );
    assert!(
        !code.contains("BackgroundCompactor"),
        "generated code must NOT link the off-writer-lock background thread"
    );

    // The live-row set is computed in generated code from id_to_row + tombstone
    // liveness (deleted ids' tombstoned marker rows are omitted — no resurrection).
    assert!(
        code.contains("for &__row in self.id_to_row.values()")
            && code.contains("self.tombstones.is_deleted(__row)"),
        "generated code computes the live keep-set from id_to_row + liveness"
    );

    // #162-C: compaction renumbers rows → reopen ONLY the column handles (stale
    // after the file rename) via `new_at_no_rehydrate`, then remap the maps in
    // place — NOT the old O(rows × indexes) `*self = Self::new_at()` rehydrate
    // rescan.  The index maps (value→id keyed) survive the renumber, so they are
    // saved + reinstalled verbatim; only id_to_row / id_versions are remapped.
    assert!(
        code.contains("*self = Self::new_at_no_rehydrate(&__root);"),
        "compact() reopens column handles only (no full rehydrate rescan)"
    );
    let compact_reopen = code.find("pub fn compact(&mut self)").unwrap();
    let compact_reopen_body = &code[compact_reopen..compact_reopen + 4500];
    assert!(
        !compact_reopen_body.contains("*self = Self::new_at(&__root);"),
        "compact() must NOT full-reopen (#162-C replaced the rehydrate rescan)"
    );
    assert!(
        compact_reopen_body.contains(".partition_point(|&__r| __r < __old_row)"),
        "compact() remaps id_to_row via the dense keep-set position (in-place)"
    );

    // Auto-invoked from update AND delete (not insert — inserts create no dead
    // version) once the threshold is reached.
    assert!(
        code.contains("self.dead_since_compaction += 1;")
            && code.contains("if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD"),
        "update/delete count toward and trigger the auto-compaction"
    );

    // #162-A: crossing the SOFT threshold on the normal write path only RECORDS
    // that a compaction is due (`compaction_due`) — it does NOT stall the write
    // with an inline compact.  A HARD ceiling still forces an inline compaction as
    // a growth-bounding safety net if `maintain()` is never called.
    assert!(
        code.contains("const COMPACTION_DEAD_CEILING_FACTOR: u64"),
        "generated hard-ceiling factor constant (#162-A safety net)"
    );
    assert!(
        code.contains("self.compaction_due = true;"),
        "soft threshold defers by setting compaction_due, not compacting inline"
    );
    assert!(
        code.contains("COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR"),
        "hard ceiling forces an inline compaction as the growth bound"
    );

    // #162-A: `maintain()` runs the deferred reclaim off the hot write turn.
    assert!(
        code.contains("pub fn maintain(&mut self)"),
        "generated per-model + Database maintain() runs deferred compaction"
    );
    assert!(
        code.contains("self.widget.maintain();") && code.contains("self.gadget.maintain();"),
        "Database::maintain() runs deferred maintenance for every model"
    );

    // Database-wide force-compact across every MODEL (junctions excluded — an
    // append-only link table accumulates no dead versions).
    assert!(
        code.contains("self.widget.compact();") && code.contains("self.gadget.compact();"),
        "Database::compact() compacts every model collection"
    );
}

#[test]
fn test_rust_generation_incremental_rehydrate() {
    // #161-B (incremental delta peer refresh) + #162-C (in-place compaction remap):
    // reopen / peer-refresh / post-compaction map rebuilds must NOT rescan every
    // row × index.  The delta path folds only the new rows via the SAME live
    // update/delete maintenance; compaction remaps physical-row references in place
    // and reinstalls the (renumber-invariant) index maps verbatim.
    let src = r#"
User {
  id: +uuid
  email: &string
  status: ^string
  region: string
  @index(status, region)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // --- #161-B: __reindex_delta folds only [from..row_count) --------------
    assert!(
        code.contains("pub fn __reindex_delta(&mut self, from: usize)"),
        "incremental delta refresh method emitted"
    );
    assert!(
        code.contains("for __r in from..__n {"),
        "delta iterates only the new rows [from..row_count)"
    );
    // Reuses the live maintenance: remove the pre-row version's keys (self.get),
    // add the folded row's keys (self.read_at) — one maintenance source, no drift.
    let delta_start = code.find("pub fn __reindex_delta").unwrap();
    // The window must clear the (verbose) per-field key-derivation blocks between
    // the remove side and the add side.
    let delta_body = &code[delta_start..delta_start + 6000];
    assert!(
        delta_body.contains("if let Some(__old_rec) = self.get(id)"),
        "delta removes the superseded version's index keys via self.get (like update)"
    );
    assert!(
        delta_body.contains("if let Some(__new_rec) = self.read_at(__r)"),
        "delta adds the folded row's index keys via self.read_at (like insert/update)"
    );
    // It must NOT clear-and-rescan (that is `__reindex_committed`, kept separate).
    assert!(
        !delta_body.contains(".clear();"),
        "delta must not clear the maps (that is the full __reindex_committed path)"
    );

    // --- #162-C: compaction remaps in place, no full rehydrate rescan ------
    let compact_start = code.find("pub fn compact(&mut self)").unwrap();
    let compact_body = &code[compact_start..compact_start + 4500];
    assert!(
        compact_body.contains("Self::new_at_no_rehydrate(&__root)"),
        "compact reopens column handles only (no O(rows × indexes) rehydrate)"
    );
    assert!(
        compact_body.contains("partition_point(|&__r| __r < __old_row)"),
        "compact remaps id_to_row to dense keep-set positions in place"
    );
    // Index maps are value→id keyed (renumber-invariant): saved + reinstalled
    // verbatim across the reopen, never rebuilt from a column scan.
    assert!(
        compact_body.contains("let __saved_status_index = std::sync::Arc::clone(&self.status_index);")
            && compact_body.contains("self.status_index = __saved_status_index;"),
        "compact preserves the (renumber-invariant) index maps across the reopen"
    );

    // The narrow-scan `new_at_no_rehydrate` exists: it recovers row_count but does
    // NOT run the rehydrate id-scan (`make_mut(&mut db.id_to_row).insert(id, i)`),
    // which is the O(rows) rebuild the in-place remap replaces.
    let nar_start = code.find("fn new_at_no_rehydrate").unwrap();
    let nar_body = &code[nar_start..nar_start + 3200];
    assert!(
        nar_body.contains("db.recover_from_wal();"),
        "new_at_no_rehydrate recovers row_count via recover_from_wal"
    );
    assert!(
        !nar_body.contains(".id_to_row).insert(id, i)"),
        "new_at_no_rehydrate skips the rehydrate id-scan (maps remapped in place)"
    );
}

#[test]
fn test_rust_generation_additive_backfill() {
    // Additive migrations (v1 Phase 4 — #92): after a field is added
    // to the schema and code is regenerated, reopening an existing data dir must
    // NOT wipe rows.  Recovery anchors on the tombstone count (the authoritative
    // committed row count) and BACKFILLS any column shorter than it (a newly-added
    // field) with the field's default, while truncating only torn/ahead columns.
    let src = r#"
Widget {
  id: +uuid
  sku: &string
  qty: u32
  note: string?
  score: f64
  active: bool
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Recovery anchors on the tombstone count, NOT the min across all columns
    // (the old `min(...)` would truncate everything to a new empty column → wipe).
    assert!(
        code.contains("let __anchor = self.tombstones.len();"),
        "recovery anchors on the authoritative tombstone row count"
    );
    assert!(
        !code.contains(".min(self.") ,
        "recovery no longer truncates to the min column length (would wipe on a new field)"
    );

    // Short columns (newly-added fields) are backfilled up to the anchor.
    assert!(
        code.contains("while self.note_col.len() < __anchor")
            && code.contains("while self.score_col.len() < __anchor"),
        "each column is backfilled up to the anchor when short"
    );

    // Correct per-type defaults: nullable string → None tag, numeric → 0,
    // f64 → 0.0, bool → false.
    assert!(
        code.contains(r#"append_tagged(0u8, "")"#),
        "nullable string backfills the None presence tag"
    );
    assert!(
        code.contains("append_f64(0.0)"),
        "f64 field backfills 0.0"
    );
    assert!(
        code.contains("append_bool(false)"),
        "bool field backfills false"
    );

    // Torn/ahead columns are still truncated to the anchor.
    assert!(
        code.contains("truncate_to_rows(__anchor)"),
        "torn/ahead columns truncate down to the anchor"
    );
}

#[test]
fn test_api_generation_list_endpoint() {
    // Real list endpoint (#90/#160): filter the narrow scan with the generated
    // closed-set matcher, sort with the generated per-model comparator, then
    // full-materialize only the page; paginate with the schema-agnostic
    // query-params substrate.
    let src = r#"
User {
  id: +uuid
  name: string
  age: u32
  score: f64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // No stub: the handler fetches real rows and filters/sorts/paginates.
    assert!(!code.contains(r#"json!({ "data": [] })"#), "list stub is gone");
    // #160: the live list filters the narrow scan through the SAME closed-set
    // matcher as the change-feed / live-query paths (no second predicate parser).
    // #224: that matcher now runs on the BORROWED scan view, during the scan — the
    // "one predicate source" guarantee is unchanged; only the operand view is.
    assert!(
        code.contains("|r| __user_scan_matches(r, &params)"),
        "list filters the narrow scan via the generated closed-set matcher (no second parser)"
    );
    // The `?as_of` snapshot path keeps the full-record read + the same closed-set
    // filter (`user_event_matches`).
    assert!(
        code.contains("user_event_matches(r, &params)"),
        "as_of snapshot path reuses the full-record closed-set filter"
    );
    assert!(
        code.contains("forgedb_query_params::QueryParams::from_map")
            && code.contains("qp.pagination.apply(&rows)"),
        "query-params substrate parses + clamps pagination"
    );
    // Generated per-model sort comparator: Ord for `age`, partial_cmp for `f64`.
    assert!(code.contains("fn user_apply_sort("), "generated per-model sort fn");
    assert!(
        code.contains("a.age.cmp(&b.age)"),
        "Ord field sorted with cmp"
    );
    assert!(
        code.contains("a.score.partial_cmp(&b.score)"),
        "f64 field sorted with partial_cmp"
    );
}

#[test]
fn test_api_generation_list_envelope() {
    // #229: the list envelope and the point reads serialize from their own types
    // instead of being routed through a `serde_json::Value` tree, which cloned
    // every string in the response before axum serialized it again.
    //
    // This is link 1 of the guard — it pins WHAT IS EMITTED. Link 2 is
    // `tests/api_wire_test.rs`, which compiles this output, boots the router, and
    // pins the bytes that emission puts on the socket. Both must move together.
    let src = r#"
enum Status { Draft, Published }

Post {
  @projection(card: title, status)
  id: +uuid
  title: string
  body: string
  status: Status
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // The envelope is emitted ONCE per file (schema-blind — it names no model and
    // no field), generic over the row type because the `?projection=` arms each
    // carry a different one. `__`-prefixed: this file does `use super::*`, so an
    // unprefixed name would be one PascalCase model away from a collision.
    assert!(
        flat.contains("struct __ListEnvelope<'a, T: serde::Serialize> { data: &'a [T], total: usize, limit: usize, offset: usize, }"),
        "the envelope is a generic struct borrowing the page, fields in wire order"
    );
    assert_eq!(
        code.matches("struct __ListEnvelope").count(),
        1,
        "emitted once for the whole file, not per model"
    );

    // The list body borrows the page straight into it — no intermediate `Value`.
    assert!(
        flat.contains("Json(__ListEnvelope { data: &page, total, limit: qp.pagination.limit, offset: qp.pagination.offset, })"),
        "the list handler borrows the page into the envelope"
    );
    // The projection arm builds a TYPED page and borrows that into the same
    // envelope — it used to `serde_json::to_value` each row into a `Vec<Value>`.
    assert!(
        flat.contains("let __data: Vec<super::PostCard> = page .iter() .map(|r| super::PostCard {"),
        "the projection list arm collects a typed page"
    );
    assert!(
        flat.contains("Json(__ListEnvelope { data: &__data,"),
        "the projection list arm borrows its typed page into the same envelope"
    );

    // Both handlers now return `Response` — success and error bodies have
    // different types, and the projection arms differ from each other again.
    assert!(
        flat.contains("async fn list_post( Query(params): Query<HashMap<String, String>>, State(db): State<Arc<RwLock<super::Database>>>, ) -> Response"),
        "the list handler returns Response"
    );
    assert!(
        flat.contains("async fn get_post( Path(id): Path<String>, Query(params): Query<HashMap<String, String>>, State(db): State<Arc<RwLock<super::Database>>>, ) -> Response"),
        "the get handler returns Response"
    );

    // Point reads serialize the record itself.
    assert!(
        flat.contains("Some(record) => (StatusCode::OK, Json(record)).into_response()"),
        "the point read serializes the record directly"
    );
    assert!(
        flat.contains("Some(r) => (StatusCode::OK, Json(r)).into_response()"),
        "the projected point read serializes the projection struct directly"
    );

    // Negative: nothing on a read path builds a `Value` tree any more. The error
    // bodies deliberately stay `json!` objects (single small objects, #229 non-goal).
    assert!(
        !code.contains("serde_json::to_value"),
        "no read path routes a record through serde_json::Value"
    );
    assert!(
        !flat.contains(r#"json!({ "data""#),
        "the envelope is no longer built by the json! macro"
    );
    assert!(
        flat.contains(r#"json!({ "error" : "not found" })"#),
        "error bodies are unchanged"
    );
}

#[test]
fn test_rust_generation_changefeed_emits() {
    // Change notifications (#62 Direction A): generated insert()/link_* emit a
    // FIELD-BLIND (model, row_index) signal into a shared substrate ChangeFeed;
    // the Database owns the feed and hands each collection a clone; typed
    // per-model event structs are generated for the WS handler to materialize.
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Each storage holds an optional shared feed, attached by Database::new().
    assert!(
        code.contains("changefeed: Option<forgedb_changefeed::ChangeFeed>")
            || code.contains("changefeed: Option<forgedb_changefeed :: ChangeFeed>"),
        "each storage must hold an optional change feed"
    );
    assert!(
        code.contains("pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed)")
            || code.contains("pub fn attach_changefeed(&mut self, feed: forgedb_changefeed :: ChangeFeed)"),
        "storages must expose attach_changefeed"
    );

    // insert() emits a field-blind signal carrying the model NAME (a &'static str)
    // and the row index — never a field value.
    assert!(
        code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed::ChangeKind::Inserted)")
            || code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed :: ChangeKind :: Inserted)"),
        "Post insert must emit an Inserted signal"
    );
    // M2M link emits a Linked signal under the junction name.
    assert!(
        code.contains("ChangeKind::Linked") || code.contains("ChangeKind :: Linked"),
        "M2M link must emit a Linked signal"
    );
    assert!(
        code.contains("\"post_tag_link\""),
        "the link emit carries the junction name, not a field"
    );

    // Database owns the shared feed and attaches a clone to every collection.
    assert!(
        code.contains("pub changefeed: forgedb_changefeed::ChangeFeed")
            || code.contains("pub changefeed: forgedb_changefeed :: ChangeFeed"),
        "Database owns the shared change feed"
    );
    assert!(
        code.contains("forgedb_changefeed::ChangeFeed::new(1024)")
            || code.contains("forgedb_changefeed :: ChangeFeed :: new(1024)"),
        "Database::new creates the feed with the default buffer"
    );
    assert!(
        code.contains("post.attach_changefeed(changefeed.clone())"),
        "each collection gets a clone of the shared feed"
    );

    // Typed per-model event structs for the WS handler (#62 insert + #66 mutation).
    assert!(code.contains("pub struct PostInserted"), "typed insert event struct");
    assert!(code.contains("pub struct PostUpdated"), "typed update event struct (#66)");
    assert!(code.contains("pub struct PostDeleted"), "typed delete event struct (#66)");
    assert!(
        code.contains("pub post: Post"),
        "the event struct carries the typed record"
    );

    // #66: generated update()/delete() emit field-blind Updated/Deleted signals.
    assert!(
        code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed::ChangeKind::Updated)")
            || code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed :: ChangeKind :: Updated)"),
        "Post update must emit an Updated signal at the new row"
    );
    assert!(
        code.contains("feed.emit(\"Post\", deleted_row, forgedb_changefeed::ChangeKind::Deleted)")
            || code.contains("feed.emit(\"Post\", deleted_row, forgedb_changefeed :: ChangeKind :: Deleted)"),
        "Post delete must emit a Deleted signal carrying the pre-delete row"
    );

    // read_at is public so the WS handler can materialize by row index.
    assert!(
        code.contains("pub fn read_at(&self, row_index: usize) -> Option<Post>"),
        "read_at must be public for change-feed materialization"
    );
}

#[test]
fn test_rust_generation_replication_broker() {
    // Durable replication (#82 Direction C): alongside the best-effort change-feed
    // emit, each mutation is recorded to a durable, offset-addressed broker so a
    // resumable follower (#110) can replay from a watermark.  The broker stays
    // FIELD-BLIND — it is handed the model NAME (opaque tag), the row index, the
    // kind, and the opaque serialized row bytes; it never decodes a field.
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    // Replication defaults OFF (#130, G6 sanctioned exception): generate WITH it
    // enabled so the broker-attach path is emitted and asserted here.
    let code = RustGenerator::generate_with_config(&schema, 1, GenConfig::legacy_with_replication())
        .unwrap()
        .code;
    // RustGenerator emits a raw token stream (spaces around `::`/`<`/`>`); compare
    // against a whitespace-stripped copy so needles stay readable.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // Each storage + the Database hold the optional shared broker. (prettyplease
    // may wrap the long generic with a trailing comma, so match the inner type.)
    assert!(
        flat.contains("broker:Option<")
            && flat.contains(
                "std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>"
            ),
        "each storage/Database must hold an optional shared durable broker"
    );
    assert!(flat.contains("fnattach_broker"), "storages must expose attach_broker");

    // The mutation sites record to the broker with the OPAQUE serialized row bytes
    // — never a decoded field.
    assert!(flat.contains(".record("), "mutations must record to the durable broker");
    assert!(
        flat.contains("serde_json::to_vec(&record).unwrap_or_default()"),
        "the broker is handed the opaque serialized row bytes (field-blind)"
    );
    // All four kinds carry through the broker (insert/update/delete/link).
    for kind in ["Inserted", "Updated", "Deleted", "Linked"] {
        let needle = format!("forgedb_changefeed::ChangeKind::{}", kind);
        assert!(flat.contains(&needle), "broker record must carry ChangeKind::{}", kind);
    }
    // The link record carries the 32-byte opaque pair, no field.
    assert!(
        flat.contains("left.as_bytes()") && flat.contains("right.as_bytes()"),
        "the link record carries the opaque left++right uuid bytes"
    );

    // Database open_at creates a durable log under the data root.
    assert!(
        flat.contains("forgedb_changefeed::durable::DurableBroker::open("),
        "open_at opens a durable broker log"
    );
    assert!(
        flat.contains("\"_replication.log\""),
        "the broker log lives under the data root"
    );
    assert!(
        flat.contains("post.attach_broker(broker.clone())"),
        "each collection gets a clone of the shared broker on open_at"
    );

    // Identity red line: the broker never branches on a decoded field.
    assert!(
        !flat.contains("matchmodel_name"),
        "the broker must never match on the model name to decode a field"
    );
}

#[test]
fn test_gen_config_knobs() {
    // Configurable runtime behavior (epic #126): the generate-time knobs in
    // GenConfig tailor the emitted consts / branches. Default reproduces today's
    // output byte-for-byte EXCEPT the replication broker (default OFF, #130 — the
    // G6 sanctioned exception).
    let src = r#"
Post {
  id: +uuid
  title: string
  author: *Author
}

Author {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();

    let flatten = |code: &str| -> String { code.chars().filter(|c| !c.is_whitespace()).collect() };

    // --- Default: replication OFF (#130), all other consts at their prior values.
    let default_code = RustGenerator::generate(&schema).unwrap().code;
    let d = flatten(&default_code);
    assert!(
        !d.contains("DurableBroker::open("),
        "default output must NOT attach the durable broker (#130 default OFF)"
    );
    assert!(
        d.contains("constWAL_CHECKPOINT_INTERVAL:u64=1000"),
        "default WAL checkpoint interval is 1000 (#131)"
    );
    assert!(
        d.contains("constCOMPACTION_DEAD_THRESHOLD:u64=1000"),
        "default compaction threshold is 1000 (#133)"
    );
    assert!(
        d.contains("constMAX_CASCADE_DEPTH:u32=64"),
        "default cascade depth is 64 (#150)"
    );
    assert!(
        d.contains("forgedb_wal::FsyncPolicy::Always"),
        "default fsync policy is Always (#129)"
    );
    assert!(
        d.contains("ChangeFeed::new(1024)"),
        "default changefeed capacity is 1024 (#135)"
    );
    assert!(
        d.contains("constDEFAULT_TXN_RETRIES:u32=3"),
        "default optimistic-transaction retry count is 3 (#146)"
    );
    // Default auto-compaction trigger is emitted (#134 ON).
    assert!(
        d.contains("self.dead_since_compaction>=COMPACTION_DEAD_THRESHOLD"),
        "default emits the auto-compaction trigger (#134 ON)"
    );

    // --- Fully custom config: every knob honored.
    let cfg = GenConfig {
        replication: true,
        fsync: forgedb_codegen::FsyncMode::Never,
        wal_checkpoint_interval: 500,
        compaction: false,
        compaction_threshold: 250,
        changefeed_capacity: 64,
        max_cascade_depth: 8,
        txn_max_retries: 9,
        page_default_limit: 25,
        page_max_limit: 500,
        metrics: true,
        wasm_commit_debounce_ms: 250,
        wasm_commit_max_frames: 100,
        replication_log_retention: 0,
    };
    let custom = RustGenerator::generate_with_config(&schema, 1, cfg).unwrap().code;
    let c = flatten(&custom);
    assert!(
        c.contains("DurableBroker::open("),
        "replication=true attaches the durable broker (#130 ON)"
    );
    assert!(
        c.contains("constWAL_CHECKPOINT_INTERVAL:u64=500"),
        "WAL checkpoint interval is configurable (#131)"
    );
    assert!(
        c.contains("constCOMPACTION_DEAD_THRESHOLD:u64=250"),
        "compaction threshold is configurable (#133)"
    );
    assert!(
        c.contains("constMAX_CASCADE_DEPTH:u32=8"),
        "cascade depth is configurable (#150)"
    );
    assert!(
        c.contains("forgedb_wal::FsyncPolicy::Never"),
        "fsync policy is configurable to Never (#129)"
    );
    assert!(
        c.contains("ChangeFeed::new(64)"),
        "changefeed capacity is configurable (#135)"
    );
    assert!(
        c.contains("constDEFAULT_TXN_RETRIES:u32=9"),
        "optimistic-transaction retry count is configurable (#146)"
    );
    assert!(
        c.contains("forgedb_changefeed::durable::FsyncPolicy::Never"),
        "the durable broker fsync follows the fsync knob (#136)"
    );
    // Tier A #134: compaction=false OMITS the auto-compaction trigger.
    assert!(
        !c.contains("self.dead_since_compaction>=COMPACTION_DEAD_THRESHOLD"),
        "compaction=false omits the auto-compaction trigger (#134 Tier A)"
    );
    // The dead-row counter is still incremented (kept live for the explicit path).
    assert!(
        c.contains("self.dead_since_compaction+=1"),
        "the dead-row counter stays live even with auto-compaction off (#134)"
    );
}

#[test]
fn test_rust_generation_replication_log_retention() {
    // #137 (epic #126, Tier B): maintain() prunes the durable replication log to
    // the last N offsets ONLY when replication is on AND retention > 0. Default
    // (or replication off) emits no prune — byte-identical.
    let src = r#"
Post {
  id: +uuid
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let flatten = |code: &str| -> String { code.chars().filter(|c| !c.is_whitespace()).collect() };

    // replication ON but retention 0 (default): no prune.
    let off = flatten(&RustGenerator::generate_with_config(&schema, 1, GenConfig::legacy_with_replication()).unwrap().code);
    assert!(
        !off.contains(".prune_through("),
        "retention 0 emits no prune (#137 default byte-identical)"
    );

    // replication ON + retention 4096: prune the broker to the last 4096 offsets.
    let cfg = GenConfig {
        replication: true,
        replication_log_retention: 4096,
        ..GenConfig::DEFAULT
    };
    let on = flatten(&RustGenerator::generate_with_config(&schema, 1, cfg).unwrap().code);
    assert!(
        on.contains(".prune_through(__wm.saturating_sub(4096))"),
        "retention 4096 prunes to the last 4096 offsets in maintain() (#137)"
    );

    // retention set but replication OFF: still no prune (no broker exists).
    let cfg_no_repl = GenConfig {
        replication: false,
        replication_log_retention: 4096,
        ..GenConfig::DEFAULT
    };
    let no_repl = flatten(&RustGenerator::generate_with_config(&schema, 1, cfg_no_repl).unwrap().code);
    assert!(
        !no_repl.contains(".prune_through("),
        "retention without replication emits no prune (#137)"
    );
}

#[test]
fn test_rust_generation_index_key_hoisting() {
    // #157 part A: the record is serialized once (WAL borrows, broker moves).
    // #157 part B: a field in ≥2 index structures (single index + composite) has
    // its index key derived ONCE per mutation and reused across both.
    let src = r#"
User {
  id: +uuid
  email: &string
  status: ^string
  region: string
  @index(status, region)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // Part A: exactly one serialize per mutation, shared. The WAL extends its
    // framed payload with the shared buffer; no second serialize call remains for
    // the broker (it moves `__record_json`).
    assert!(
        flat.contains("let__record_json=serde_json::to_vec(&record)"),
        "the record is serialized once into __record_json (#157 A)"
    );
    assert!(
        flat.contains("__wal_payload.extend_from_slice(&__record_json)"),
        "the WAL reuses the shared serialized buffer (#157 A)"
    );
    // The broker MOVES the shared buffer instead of re-serializing: its `record`
    // call is passed `__record_json` (the ChangeKind + the moved bytes).
    assert!(
        flat.contains("ChangeKind::Inserted,__record_json,"),
        "the broker record moves the shared buffer, not a fresh serialize (#157 A)"
    );

    // Part B: `status` (single index ^ + composite component) is HOISTED — its key
    // is bound once per direction and reused; `region` (composite-only, one
    // structure) is NOT hoisted (stays inline).
    assert!(
        flat.contains("let__ik_add_status:String=")
            && flat.contains("let__ik_rem_status:String="),
        "the shared field's key is hoisted for both add and remove directions (#157 B)"
    );
    assert!(
        flat.contains("__ik_add_status.clone()"),
        "hoisted key is reused (cloned) across single + composite adds (#157 B)"
    );
    assert!(
        !flat.contains("let__ik_add_region:String="),
        "a single-structure field (region) is not hoisted — inline, no clone (#157 B)"
    );
    // Update groups all removes under one guarded `if let Some(__old_rec)`.
    assert!(
        flat.contains("ifletSome(__old_rec)=&__old{"),
        "update groups removes under one old-record guard (#157 B)"
    );
}

#[test]
fn test_rust_generation_replica_apply_path() {
    // Browser read-replica follower apply path (#110 Milestone C): the same
    // generated data logic recompiles for wasm and gains a follower entry point
    // that REPLAYS opaque frames through the existing insert/update/delete — no
    // second write path, and it never decodes a field to *route* (dispatch is by
    // the opaque model tag; only materialization decodes, in generated code).
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The apply error type + its reuse-the-write-path conversion.
    assert!(flat.contains("pubenumApplyError"), "must emit an ApplyError type");
    assert!(
        flat.contains("implFrom<ValidationError>forApplyError"),
        "apply reuses the write path, so ValidationError converts into ApplyError"
    );

    // Per-model follower apply, decoding the OPAQUE bytes (not a field) and
    // replaying through the same generated mutation surface.
    assert!(
        flat.contains("pubfnapply(&mut self,kind:forgedb_changefeed::ChangeKind,bytes:&[u8],)")
            || flat.contains("pubfnapply(&mutself,kind:forgedb_changefeed::ChangeKind,bytes:&[u8],)"),
        "each id-bearing model must expose a follower apply(kind, bytes)"
    );
    assert!(
        flat.contains("serde_json::from_slice(bytes)"),
        "apply decodes the opaque row bytes into the typed record"
    );
    // Replays through the existing surface — self.insert / self.update / self.delete.
    for m in ["self.insert(record)", "self.update(id,record)", "self.delete(id)"] {
        assert!(flat.contains(m), "apply must replay through the existing {m}");
    }

    // Schema-wide dispatcher, routing by the OPAQUE model tag.
    assert!(
        flat.contains("pubfnapply_frame(&mutself,ev:&forgedb_changefeed::durable::PersistedEvent,)"),
        "Database must expose apply_frame(&PersistedEvent)"
    );
    assert!(
        flat.contains("matchev.model.as_str()"),
        "apply_frame dispatches on the opaque model tag string"
    );
    assert!(flat.contains("\"Post\"=>"), "model tag arm present");
    assert!(flat.contains("\"post_tag_link\"=>"), "junction tag arm present");
    // The junction arm re-links from the opaque 32-byte pair, no field decode.
    // #266 made the decode per-endpoint (each side is its own key type), so the
    // pair is bound first and passed positionally; for this uuid/uuid junction
    // the framed width is still 32 and each half is still `Uuid::from_bytes`.
    assert!(
        flat.contains("ifev.bytes.len()==32{"),
        "the junction frame is still the two endpoint widths"
    );
    assert!(
        flat.contains("__b.copy_from_slice(&ev.bytes[0..16]);Uuid::from_bytes(__b)")
            && flat.contains("__b.copy_from_slice(&ev.bytes[16..32]);Uuid::from_bytes(__b)"),
        "each half of the opaque pair decodes as that endpoint's key"
    );
    assert!(
        flat.contains("self.post_tag_link.link(__l,__r)"),
        "junction frames re-link from the opaque left++right pair"
    );

    // Additive commit across collections.
    assert!(
        flat.contains("pubfncommit(&mutself)->std::io::Result<()>"),
        "Database + each storage must expose an additive commit()"
    );

    // IDENTITY RED LINE: apply_frame must dispatch on the model TAG, never on a
    // decoded record field.  (`match ev.model` is allowed — the tag; `match`ing a
    // decoded field would be the forbidden generic engine.)
    assert!(
        !flat.contains("matchrecord."),
        "apply_frame/apply must never branch on a decoded record field"
    );
}

#[test]
fn test_rust_generation_recover_to() {
    // Point-in-time recovery (#77): the generated Database exposes recover_to,
    // which replays durable-broker frames in (base_offset .. target] through the
    // SAME opaque-model-tag apply_frame the #110 follower uses — no second decode
    // path, the broker offset is the only ordering key, and it detaches the broker
    // during replay so replayed mutations don't re-record into the log it reads.
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The recovery entry point.
    assert!(
        flat.contains("pubfnrecover_to(&mutself,base_offset:u64,target_offset:u64,)->Result<u64,ApplyError>"),
        "Database must expose recover_to(base_offset, target_offset)"
    );
    // Reuses the durable broker's read_from — no new read path.
    assert!(
        flat.contains(".read_from(after,BATCH)"),
        "recover_to reads frames via DurableBroker::read_from"
    );
    // Replays through the SAME apply_frame (opaque model tag dispatch) — no second
    // decode path.
    assert!(
        flat.contains("self.apply_frame(ev)"),
        "recover_to must replay through the existing apply_frame, not a new path"
    );
    // Strict (base_offset .. target] window: begins after base, stops past target.
    assert!(
        flat.contains("ifev.offset>target_offset"),
        "recover_to stops at the target offset (exclusive upper bound past target)"
    );
    // Detaches the broker during replay so replayed mutations do not re-record.
    assert!(
        flat.contains(".attach_broker(None)"),
        "recover_to detaches the broker during replay so apply doesn't re-record frames"
    );
    // IDENTITY RED LINE: recovery ordering is the opaque broker offset, never a
    // decoded record field.  apply_frame already dispatches by the model tag.
    assert!(
        !flat.contains("matchrecord."),
        "recover_to must never branch on a decoded record field — offset is the only key"
    );
}

#[test]
fn test_wasm_generation_transport() {
    // Browser read-replica transport (#110 Milestone C, follow-up #3): the
    // per-schema `#[wasm_bindgen] Replica` that was hand-written in the harness
    // is now GENERATED. It exposes schema-invariant lifecycle plus a read surface
    // that MIRRORS the generated Database's reads exactly — inventing no query
    // API (the identity red line) and exposing no mutators (a read-only follower).
    let src = r#"
User {
  id: +uuid
  email: string
  posts: [Post]
  tags: [Tag]
}

Post {
  id: +uuid
  title: string
  author: *User
}

Tag {
  id: +uuid
  label: string
  users: [User]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = WasmGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The wasm-bindgen struct + schema-invariant lifecycle.
    assert!(flat.contains("#[wasm_bindgen]"), "must annotate for wasm-bindgen");
    assert!(flat.contains("pubstructReplica"), "must expose a Replica struct");
    assert!(flat.contains("pubasyncfnopen("), "lifecycle: open");
    assert!(flat.contains("js_name=applyWire"), "lifecycle: applyWire");
    assert!(flat.contains("pubasyncfncommit("), "lifecycle: commit");
    assert!(flat.contains("pubfnwatermark(&self)->u64"), "lifecycle: watermark");
    // Follower routes opaque frames through the generated apply_frame.
    assert!(
        flat.contains("apply_frame(&ev)"),
        "applyWire replays through the generated apply_frame"
    );

    // Per-model core reads for every model (get / count / all). Generated reads
    // carry a string `js_name` (interpolated); the fixed lifecycle uses the ident
    // form (e.g. `js_name = applyWire`).
    for js in [
        "js_name=\"getUser\"", "js_name=\"userCount\"", "js_name=\"allUsers\"",
        "js_name=\"getPost\"", "js_name=\"postCount\"", "js_name=\"allPosts\"",
        "js_name=\"getTag\"", "js_name=\"tagCount\"", "js_name=\"allTags\"",
    ] {
        assert!(flat.contains(js), "missing core read {js}");
    }

    // Relation traversals, mirroring the generated Database method names:
    //   forward FK (post.author -> User), reverse 1:M (user -> posts),
    //   M2M query helpers (user.tags -> Tag, tag.users -> User).
    assert!(flat.contains("js_name=\"postAuthor\""), "forward FK getter");
    assert!(flat.contains("js_name=\"userPosts\""), "reverse one-to-many getter");
    assert!(flat.contains("js_name=\"userTags\""), "M2M forward query");
    assert!(flat.contains("js_name=\"tagUsers\""), "M2M reverse query");

    // The reads call the generated surface — never a reimplemented query.
    assert!(flat.contains(".post_author(&__rec)"), "forward FK resolves via generated getter");
    assert!(flat.contains(".user_posts(__pk)"), "reverse getter calls generated method");

    // IDENTITY RED LINE: read-only follower — exposes NO mutators. Every write
    // path (insert/update/delete/link) stays in the generated Database (reached
    // only by the follower `apply_frame`, never by a JS-callable method here).
    for forbidden in [".insert(", ".update(", ".delete(", "js_name=\"linkUserTag\"", "fnlink_"] {
        assert!(
            !flat.contains(forbidden),
            "transport must expose no mutator (found {forbidden})"
        );
    }
}

#[test]
fn test_wasm_generation_async_client_and_worker() {
    // #110 follow-up #2 (engine-in-Worker): the generated main-thread async
    // `ReplicaClient` must STRICTLY MIRROR the `Replica`'s read surface (PM
    // constraint 2 — reuse the one enumerator, invent nothing), and the Worker
    // bootstrap must be STATIC + schema-agnostic (PM constraint 3).
    let src = r#"
User {
  id: +uuid
  email: string
  posts: [Post]
  tags: [Tag]
}

Post {
  id: +uuid
  title: string
  author: *User
}

Tag {
  id: +uuid
  label: string
  users: [User]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();

    let replica = WasmGenerator::generate(&schema).unwrap().code;
    let client = WasmGenerator::generate_client(&schema).unwrap().code;
    let worker = WasmGenerator::worker_bootstrap();

    // #148 (epic #126, Tier B): the auto-commit debounce/frame ceiling are
    // substituted from config into the static bootstrap. Default 250/100
    // (byte-identical); the schema-agnostic template is otherwise unchanged.
    assert!(
        worker.contains("const COMMIT_DEBOUNCE_MS = 250;")
            && worker.contains("const COMMIT_MAX_FRAMES = 100;"),
        "default worker bootstrap keeps 250ms / 100 frames (#148)"
    );
    let custom_worker = WasmGenerator::worker_bootstrap_with_config(GenConfig {
        wasm_commit_debounce_ms: 500,
        wasm_commit_max_frames: 40,
        ..GenConfig::DEFAULT
    });
    assert!(
        custom_worker.contains("const COMMIT_DEBOUNCE_MS = 500;")
            && custom_worker.contains("const COMMIT_MAX_FRAMES = 40;"),
        "worker bootstrap commit debounce/frames are configurable (#148)"
    );
    // The substitution touches ONLY those two constants — the rest of the static,
    // schema-agnostic pipe is unchanged (#110 constraint).
    assert!(
        custom_worker.contains("replica[method]") && custom_worker.contains("scheduleCommit"),
        "the bootstrap stays the same schema-agnostic pipe (#148/#110)"
    );

    // --- Client mirrors the Replica read surface exactly ---------------------
    // Every generated read the Replica exposes has a same-named async client
    // method; the client invents none.
    for name in [
        "getUser", "userCount", "allUsers", "getPost", "postCount", "allPosts",
        "getTag", "tagCount", "allTags", "postAuthor", "userPosts", "userTags", "tagUsers",
    ] {
        assert!(
            client.contains(&format!("async {name}(")),
            "client missing generated read {name}"
        );
    }

    // STRICT MIRROR: every async method the client declares (minus the fixed
    // lifecycle) must correspond to a `js_name` the Replica actually exports —
    // the client can never drift ahead of, or invent a method absent from, the
    // Replica. Lifecycle names are schema-invariant and exempt.
    let lifecycle = ["init", "open", "applyWire", "commit", "watermark", "close"];
    for line in client.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("async ") {
            let name = &rest[..rest.find('(').unwrap_or(rest.len())];
            if lifecycle.contains(&name) {
                continue;
            }
            assert!(
                replica.contains(&format!("js_name = \"{name}\"")),
                "client method `{name}` has no matching Replica read (strict-mirror violation)"
            );
        }
    }

    // IDENTITY RED LINE: read-only follower — the client exposes no mutator.
    for forbidden in ["async create", "async insert", "async update", "async delete", "async link"] {
        assert!(
            !client.contains(forbidden),
            "client must expose no mutator (found {forbidden})"
        );
    }

    // --- Worker is static + schema-agnostic (PM constraint 3) ----------------
    // Generic dispatch only; it interprets no schema.
    assert!(
        worker.contains("replica[method](...args)"),
        "worker dispatches reads generically by method name"
    );
    // It must not mention any model, field, or a `match`/route over the schema.
    for schema_token in ["User", "Post", "Tag", "email", "title", "author", "getUser", "userPosts"] {
        assert!(
            !worker.contains(schema_token),
            "worker bootstrap must be schema-agnostic (found `{schema_token}`)"
        );
    }
}

#[test]
fn test_ffi_generation_spine() {
    // Native FFI Layer-0 C-ABI spine (language bindings #51/#52/#117, Phase 2):
    // the schema-invariant lifecycle + error surface every binding hangs off.
    // Generated per schema (it links the per-schema `Database`) but its SYMBOL
    // SET is schema-invariant — no per-model op, no field name, and above all no
    // generic `forgedb_query(model, predicate)` (the removed-QueryBuilder red
    // line, acceptance constraint 1).
    let src = r#"
User {
  id: +uuid
  email: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = FfiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The spine wraps the generated Database — same shape as the wasm replica.
    assert!(flat.contains("moddatabase;"), "spine links the generated database module");
    assert!(flat.contains("usedatabase::Database;"), "spine uses the generated Database");
    assert!(flat.contains("pubstructDb"), "opaque C-ABI Db handle");
    assert!(flat.contains("pubstructForgeError"), "C-ABI error object");

    // Every pinned schema-invariant lifecycle / control symbol is present, each
    // an `extern "C"` `#[unsafe(no_mangle)]` entry point.
    for sym in [
        "forgedb_version", "forgedb_open", "forgedb_close", "forgedb_commit",
        "forgedb_checkpoint", "forgedb_compact", "forgedb_error_code",
        "forgedb_error_message", "forgedb_error_free", "forgedb_free_buffer",
    ] {
        assert!(
            flat.contains(&format!("fn{sym}(")),
            "missing pinned ABI symbol {sym}"
        );
    }
    assert!(flat.contains("extern\"C\""), "symbols are extern \"C\"");
    assert!(flat.contains("no_mangle"), "symbols are #[no_mangle]");

    // The spine calls the generated Database lifecycle — never a reimplementation.
    assert!(flat.contains("Database::open_at("), "open wraps the generated open_at");
    assert!(flat.contains(".inner.commit()"), "commit wraps the generated commit");
    assert!(flat.contains(".inner.checkpoint()"), "checkpoint wraps the generated checkpoint");
    assert!(flat.contains(".inner.compact()"), "compact wraps the generated compact");

    // PANIC DISCIPLINE: engine calls are wrapped so a panic becomes a ForgeError
    // instead of unwinding across the C-ABI boundary into a foreign caller (UB).
    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

    // IDENTITY RED LINE: no generic runtime query/filter symbol, and no schema
    // dispatch — the ABI has per-model ops (the fat tailored half) but NEVER a
    // generic `forgedb_query(model, predicate)` or a `match model` router.
    for forbidden in [
        "forgedb_query", "match model", "matchmodel", "predicate", "where(",
        "orderBy",
    ] {
        assert!(
            !flat.contains(forbidden),
            "the ABI must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_model_ops() {
    // Native FFI Phase 3 — the schema-TAILORED half of the fat C-ABI: per-model
    // OLTP row ops in the SAME generated lib.rs as the spine.  They reference the
    // generated structs + integrity wrappers by name (that is the tailoring) but
    // marshal rows/ids as OPAQUE JSON bytes decoded via serde at a compile-time
    // type — no generic query builder, no `match model` dispatch.
    let src = r#"
User {
  id: +uuid
  email: &string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}

Reading {
  id: +u64
  value: i64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = FfiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // Every identity model gets the OLTP entry points + the point-in-time (#56)
    // `_at` reads, each `extern "C"` `#[no_mangle]`, keyed by the model's snake
    // name.
    for model in ["user", "post", "reading"] {
        for op in ["insert", "get", "count", "all", "update", "delete", "get_at", "all_at"] {
            assert!(
                flat.contains(&format!("fnforgedb_{model}_{op}(")),
                "missing per-model op forgedb_{model}_{op}"
            );
        }
    }

    // The schema-invariant snapshot lifecycle spine (capture + free) exists once.
    assert!(flat.contains("fnforgedb_snapshot("), "snapshot capture entry point");
    assert!(flat.contains("fnforgedb_snapshot_free("), "snapshot free entry point");
    assert!(flat.contains(".inner.snapshot()"), "capture wraps Database::snapshot()");

    // Ops go through the generated integrity wrappers + storage reads — not a
    // reimplementation of the write/read path.
    assert!(flat.contains(".inner.create_user("), "insert uses the create_<m> integrity wrapper");
    assert!(flat.contains(".inner.update_post("), "update uses the update_<m> wrapper");
    assert!(flat.contains(".inner.delete_user("), "delete uses the delete_<m> referential wrapper");
    assert!(flat.contains(".inner.user.get("), "get uses the generated storage read");
    assert!(flat.contains(".inner.reading.row_count("), "count uses the generated row_count");

    // The `_at` reads resolve as of the captured snapshot's per-model watermark
    // (`snap.inner.<model>`), through the generated `get_at`/`all_at`.
    assert!(flat.contains(".inner.user.get_at(&snap.inner.user,"), "get_at clamps to the model's watermark");
    assert!(flat.contains(".inner.post.all_at(&snap.inner.post)"), "all_at clamps to the model's watermark");

    // Rows decode into the GENERATED struct at a compile-time type (serde over
    // opaque JSON bytes) — the schema-tailored, identity-clean marshalling.
    assert!(flat.contains("database::User"), "records decode into the generated User struct");
    assert!(flat.contains("database::Post"), "records decode into the generated Post struct");
    assert!(flat.contains("serde_json::from_slice"), "opaque JSON bytes → typed record via serde");

    // Integer-PK models thread their own id type through the id decode.
    assert!(flat.contains("id:u64"), "integer-PK model decodes a u64 id (not a forced Uuid)");

    // A rejected write becomes a validation error, and every engine call is
    // catch_unwind-guarded (an unwind across extern \"C\" is UB).
    assert!(flat.contains("FORGEDB_ERR_VALIDATION"), "integrity failures map to a validation code");
    assert!(flat.contains("catch_unwind"), "per-model engine calls are catch_unwind-guarded");

    // IDENTITY: still no generic query surface / runtime schema dispatch.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "per-model ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_relation_ops() {
    // Native FFI Phase 3 (relation traversal): the C-ABI getters that mirror the
    // generated `Database` traversal methods one-for-one — forward FK, reverse
    // 1:M, and M2M link/unlink/query — each a fixed generated edge walk keyed on
    // an id, never a runtime predicate.
    let src = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  label: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = FfiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // Forward FK getter: resolve a Post's *User author.
    assert!(flat.contains("fnforgedb_post_author("), "forward-FK getter forgedb_post_author");
    assert!(flat.contains(".inner.post_author("), "wraps the generated post_author getter");
    assert!(flat.contains(".inner.post.get("), "forward-FK fetches the source record by id first");

    // Reverse 1:M getter: all Posts of a User.
    assert!(flat.contains("fnforgedb_user_posts("), "reverse-1:M getter forgedb_user_posts");
    assert!(flat.contains(".inner.user_posts("), "wraps the generated user_posts getter");

    // M2M: link/unlink + both query directions.
    assert!(flat.contains("fnforgedb_link_post_tag("), "M2M link forgedb_link_post_tag");
    assert!(flat.contains("fnforgedb_unlink_post_tag("), "M2M unlink forgedb_unlink_post_tag");
    assert!(flat.contains("fnforgedb_post_tags("), "M2M forward query forgedb_post_tags");
    assert!(flat.contains("fnforgedb_tag_posts("), "M2M reverse query forgedb_tag_posts");
    assert!(flat.contains(".inner.link_post_tag("), "wraps the generated link_post_tag");
    assert!(flat.contains(".inner.post_tags("), "wraps the generated post_tags query");

    // The one snapshot-scoped traversal (#56) is now emitted, mirroring
    // `Database::post_tags_at` and clamping both sides of the join to `snap`.
    assert!(flat.contains("fnforgedb_post_tags_at("), "snapshot `_at` M2M traversal getter");
    assert!(flat.contains(".inner.post_tags_at(&snap.inner,"), "wraps the generated post_tags_at");
    // The reverse direction has no `_at` on Database (only the forward M2M is
    // snapshot-scoped), so no reverse `_at` wrapper is emitted.
    assert!(!flat.contains("forgedb_tag_posts_at"), "only the forward M2M traversal is snapshot-scoped");

    // Every engine call is catch_unwind-guarded.
    assert!(flat.contains("catch_unwind"), "traversal engine calls are catch_unwind-guarded");

    // IDENTITY: fixed edge walks, no generic query surface.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "relation ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_async_ops() {
    // Native FFI Phase 3 (`_async` completion bridge): each identity model gets
    // `_async` variants of the OLTP ops in the SAME lib.rs, over a schema-INVARIANT
    // spine (one background worker + a caller-registered completion callback keyed
    // by `token`).  The async path calls the SAME generated integrity wrappers /
    // storage reads as the sync path — no second write/read path, no generic query.
    let src = r#"
User {
  id: +uuid
  email: &string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}

Reading {
  id: +u64
  value: i64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = FfiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The schema-invariant async spine: callback registration + the executor +
    // the completion + the Send-asserted db wrapper.
    assert!(flat.contains("fnforgedb_set_completion_callback("), "async completion-callback registration");
    assert!(flat.contains("typeForgeCompletion"), "the completion callback type is defined");
    assert!(flat.contains("staticCOMPLETION_CB"), "the process-wide callback slot exists");
    assert!(flat.contains("structSendDb"), "the Send db-pointer wrapper exists");
    assert!(flat.contains("unsafeimplSendforSendDb"), "SendDb is unsafe-Send for the worker thread");
    assert!(flat.contains("fnfire_completion("), "the completion delivery helper exists");
    assert!(flat.contains("fnasync_executor("), "the single background worker executor exists");
    assert!(flat.contains("fnspawn_async"), "jobs are enqueued on the worker");
    // Static proof the engine is Send (a non-Send engine fails the FFI build).
    assert!(flat.contains("assert_send::<Db>("), "Db is statically asserted Send");

    // Every identity model gets every OLTP op's `_async` variant, each taking the
    // completion `token: u64` and returning void, keyed by the model's snake name.
    for model in ["user", "post", "reading"] {
        for op in ["get", "all", "count", "insert", "update", "delete"] {
            assert!(
                flat.contains(&format!("fnforgedb_{model}_{op}_async(")),
                "missing async op forgedb_{model}_{op}_async"
            );
        }
    }
    assert!(flat.contains("token:u64"), "async ops carry the completion token");

    // The async path routes through the SAME generated wrappers/reads as the sync
    // path — never a second write/read implementation.
    assert!(flat.contains(".inner.create_user("), "async insert uses the create_<m> integrity wrapper");
    assert!(flat.contains(".inner.update_post("), "async update uses the update_<m> wrapper");
    assert!(flat.contains(".inner.delete_user("), "async delete uses the delete_<m> referential wrapper");
    assert!(flat.contains(".inner.user.get(id)"), "async get uses the generated storage read");
    assert!(flat.contains(".inner.reading.row_count("), "async count uses the generated row_count");

    // The work is offloaded (spawn_async) and every engine call stays
    // catch_unwind-guarded so a panic keeps the worker alive as a completion.
    assert!(flat.contains("spawn_async(move||"), "async ops enqueue their engine call off the caller thread");
    assert!(flat.contains("catch_unwind"), "async engine calls are catch_unwind-guarded");

    // A rejected write is delivered through the callback (a validation code), not a
    // panic — same integrity mapping as the sync ops.
    assert!(flat.contains("FORGEDB_ERR_VALIDATION"), "async integrity failures map to a validation code");

    // IDENTITY: still no generic query surface / runtime schema dispatch.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "async ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_arrow_export() {
    // Native FFI Arrow columnar export (the zero-copy selling point): each
    // identity model gets, per Arrow-exportable non-null fixed-width column, a
    // `forgedb_<m>_<f>_export_arrow` filling the Arrow C-Data-Interface structs.
    // The exportable set + formats come from the shared `arrow_export_format`
    // source of truth; bool / nullable / variable columns are skipped (they need
    // a transform).  Still no generic query surface.
    let src = r#"
User {
  id: +uuid
  age: i64
  score: f64
  active: bool
  bio: string?
  created_at: timestamp
}

Post {
  id: +uuid
  views: i32
  author: *User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = FfiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The schema-invariant Arrow spine: the two C-Data-Interface structs, the
    // owner box + both release callbacks, and the fill helper.
    assert!(flat.contains("structArrowSchema"), "the Arrow schema struct is defined");
    assert!(flat.contains("structArrowArray"), "the Arrow array struct is defined");
    assert!(flat.contains("structArrowArrayOwner"), "the export-buffer owner box is defined");
    assert!(flat.contains("fnarrow_array_release("), "the array release callback exists");
    assert!(flat.contains("fnarrow_schema_release("), "the schema release callback exists");
    assert!(flat.contains("fnfill_arrow_primitive("), "the fill helper exists");
    // Release reclaims the owner box; a non-null validity buffer count of 2.
    assert!(flat.contains("Box::from_raw"), "the array release reclaims + drops the owner box");
    assert!(flat.contains("n_buffers:2"), "a primitive array has two buffers (validity + data)");
    // The buffer is carried by the alias-or-gather-transparent ColumnExport, so
    // the release path frees a copy OR munmaps an alias with no ABI difference.
    assert!(
        flat.contains("forgedb_storage::ColumnExport"),
        "the export buffer is a forgedb_storage::ColumnExport (mmap alias or gathered copy)"
    );

    // Every Arrow-exportable column gets an export op, keyed by the model's snake
    // name and the field name.
    for sym in [
        "forgedb_user_id_export_arrow(",       // uuid -> w:16
        "forgedb_user_age_export_arrow(",      // i64  -> l
        "forgedb_user_score_export_arrow(",    // f64  -> g
        "forgedb_user_created_at_export_arrow(", // timestamp -> l
        "forgedb_post_id_export_arrow(",       // uuid -> w:16
        "forgedb_post_views_export_arrow(",    // i32  -> i
        "forgedb_post_author_export_arrow(",   // required FK (uuid) -> w:16
    ] {
        assert!(flat.contains(&format!("fn{sym}")), "missing Arrow export op {sym}");
    }

    // The Arrow format strings are baked from the schema at codegen time (never a
    // runtime column list): w:16 (uuid / FK) and g (f64) are distinctive.
    assert!(flat.contains("w:16"), "uuid / FK columns export as Arrow FixedSizeBinary(16)");

    // Columns needing a transform are SKIPPED (bool bit-packs; nullable/variable
    // need a validity/offset transform — a later increment).
    assert!(!flat.contains("forgedb_user_active_export_arrow"), "bool column must be skipped");
    assert!(!flat.contains("forgedb_user_bio_export_arrow"), "nullable string column must be skipped");

    // The export routes through the generated live-set + column export (the
    // live-set decision stays in generated code; the alias-vs-gather decision is
    // the storage primitive's), then fills the Arrow structs.
    assert!(flat.contains(".inner.user.export_live_indices()"), "the live row set comes from generated code");
    assert!(flat.contains(".inner.user.export_col_age("), "the column is exported via the generated export_col_<f>");
    assert!(flat.contains("fill_arrow_primitive("), "the exported buffer fills the Arrow structs");
    assert!(flat.contains("catch_unwind"), "the export is catch_unwind-guarded");

    // IDENTITY: no generic query surface / runtime schema dispatch.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "Arrow export must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_pyo3_generation_binding() {
    // PyO3 Python binding (#51) — the ergonomic per-runtime wrapper over the SAME
    // generated `database.rs`.  A `#[pyclass]` row type per identity model + a
    // `ForgeDb` whose methods mirror the generated CRUD 1:1, calling the integrity
    // wrappers + storage reads by name.  Rows marshal natively through the struct's
    // own serde via pythonize — no generic query builder, no `match model`.
    let src = r#"
User {
  id: +uuid
  email: &string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  label: string
  posts: [Post]
}

Reading {
  id: +u64
  value: i64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = PyO3Generator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // One `#[pyclass]` row type per identity model, named for the model (the
    // Python-visible name), a newtype over the generated struct.
    for model in ["User", "Post", "Reading"] {
        assert!(
            flat.contains(&format!("#[pyclass(name=\"{model}\")]")),
            "missing #[pyclass] row type for {model}"
        );
        assert!(
            flat.contains(&format!("structPy{model}")),
            "row class Py{model} is a newtype over the generated struct"
        );
    }

    // The `ForgeDb` handle + its schema-invariant lifecycle.
    assert!(flat.contains("structForgeDb"), "the ForgeDb handle exists");
    assert!(flat.contains("Database::open_at("), "open wraps Database::open_at");
    assert!(flat.contains("self.inner.commit()"), "commit wraps the generated commit");

    // Per-model CRUD, each keyed by the model's snake name, going through the
    // generated integrity wrappers + storage reads (not a reimplementation).
    for m in ["user", "post", "reading"] {
        for op in ["create", "get", "all", "count", "update", "delete"] {
            assert!(
                flat.contains(&format!("fn{op}_{m}")),
                "missing per-model method {op}_{m}"
            );
        }
    }
    assert!(flat.contains("self.inner.create_user("), "create uses the create_<m> integrity wrapper");
    assert!(flat.contains("self.inner.update_post("), "update uses the update_<m> wrapper");
    assert!(flat.contains("self.inner.delete_user("), "delete uses the delete_<m> referential wrapper");
    assert!(flat.contains("self.inner.user.get("), "get uses the generated storage read");
    assert!(flat.contains("self.inner.reading.row_count("), "count uses the generated row_count");
    assert!(flat.contains("self.inner.post.all()"), "all uses the generated storage read");

    // Rows/ids marshal through the generated struct's serde via pythonize (native
    // Python objects, one source of truth — no second per-field matrix).
    assert!(flat.contains("database::User"), "rows decode into the generated User struct");
    assert!(flat.contains("pythonize::depythonize"), "inbound rows/ids via pythonize");
    assert!(flat.contains("pythonize::pythonize"), "outbound rows/ids via pythonize");

    // Integrity failures raise the Python-visible ForgeDbError; engine calls are
    // catch_unwind-guarded (a panic must not unwind into the interpreter).
    assert!(flat.contains("ForgeDbError"), "errors surface as a Python ForgeDbError");
    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

    // The module registers every row class + the handle.
    assert!(flat.contains("#[pymodule]"), "a #[pymodule] entry point is generated");
    assert!(flat.contains("m.add_class::<ForgeDb>()"), "ForgeDb is registered");
    assert!(flat.contains("m.add_class::<PyUser>()"), "the User row class is registered");

    // Typed field getters (Phase 5b follow-up) — per-field `#[getter]` on each
    // `#[pyclass]` row type with concrete return types where possible.
    // uuid fields → `String` (hyphenated UUID); string fields → `String`;
    // i64 fields → `i64`; the Reading.id (u64) → `u64`.
    // Simple typed getters do NOT take a `py: Python<'py>` parameter.
    assert!(
        flat.contains("fnid(&self)->PyResult<String>"),
        "uuid id getter returns String"
    );
    assert!(
        flat.contains("fnemail(&self)->PyResult<String>"),
        "string email getter returns String"
    );
    assert!(
        flat.contains("fnvalue(&self)->PyResult<i64>"),
        "i64 value getter returns i64"
    );
    // Reading id is u64 PK — typed getter returns u64.
    assert!(
        flat.contains("fnid(&self)->PyResult<u64>"),
        "u64 id getter returns u64"
    );
    // title is a string field on Post.
    assert!(
        flat.contains("fntitle(&self)->PyResult<String>"),
        "string title getter returns String"
    );
    // FK field: Post.author is *User (RequiredReference → Uuid → String).
    assert!(
        flat.contains("fnauthor(&self)->PyResult<String>"),
        "required FK (Uuid) getter returns String"
    );
    // `__repr__` and `to_dict` remain on all row classes.
    assert!(flat.contains("fn__repr__"), "__repr__ is generated on every row class");
    assert!(flat.contains("fnto_dict"), "to_dict is generated on every row class");

    // Relation traversal (Phase 5b) — forward FK / reverse 1:M / M2M, mirroring the
    // generated `Database` getters by name, marshalling into the Py row classes.
    assert!(
        flat.contains("fnpost_author(&self,id:&Bound<'_,PyAny>)->PyResult<Option<PyUser>>"),
        "forward FK post_author resolves to Option<PyUser>"
    );
    assert!(
        flat.contains("self.inner.post.get(id).and_then(|__rec|self.inner.post_author(&__rec))"),
        "forward FK fetches the source then resolves the generated getter"
    );
    assert!(
        flat.contains("fnuser_posts(&self,id:&Bound<'_,PyAny>)->PyResult<Vec<PyPost>>"),
        "reverse 1:M user_posts returns Vec<PyPost>"
    );
    assert!(flat.contains("fnlink_post_tag(&mutself"), "M2M link_post_tag exists");
    assert!(
        flat.contains("fnunlink_post_tag(&mutself") && flat.contains("PyResult<bool>"),
        "M2M unlink_post_tag returns bool"
    );
    assert!(
        flat.contains("fnpost_tags(&self,id:&Bound<'_,PyAny>)->PyResult<Vec<PyTag>>"),
        "M2M forward query post_tags returns Vec<PyTag>"
    );
    assert!(
        flat.contains("fntag_posts(&self,id:&Bound<'_,PyAny>)->PyResult<Vec<PyPost>>"),
        "M2M reverse query tag_posts returns Vec<PyPost>"
    );

    // Arrow columnar export (Phase 5b) — a schema-invariant ArrowColumn #[pyclass]
    // implementing the Arrow PyCapsule protocol, + per-exportable-column methods.
    assert!(flat.contains("structArrowColumn"), "the ArrowColumn class exists");
    assert!(flat.contains("fn__arrow_c_array__"), "ArrowColumn implements the Arrow PyCapsule protocol");
    assert!(flat.contains("\"arrow_array\""), "the array capsule is named arrow_array");
    assert!(flat.contains("\"arrow_schema\""), "the schema capsule is named arrow_schema");
    assert!(flat.contains("m.add_class::<ArrowColumn>()"), "ArrowColumn is registered");
    // Exportable columns get an `_arrow` method (Reading.value = i64, Reading.id = u64).
    assert!(
        flat.contains("fnreading_value_arrow(&self)->PyResult<ArrowColumn>"),
        "the exportable i64 column gets a zero-copy Arrow method"
    );
    assert!(flat.contains("fnreading_id_arrow(&self)->PyResult<ArrowColumn>"), "the u64 PK column is exportable");
    assert!(
        flat.contains("self.inner.reading.export_live_indices()") && flat.contains(".export_col_value("),
        "the export computes the live set in generated code + gathers the one column"
    );
    // A non-exportable column (variable-length string) gets NO Arrow method.
    assert!(!flat.contains("post_title_arrow"), "a string column is not Arrow-exportable");

    // IDENTITY: no generic query surface / runtime schema dispatch.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "the PyO3 binding must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_napi_generation_binding() {
    // NAPI-RS Node/Bun binding (#52/#117) — the ergonomic per-runtime wrapper over
    // the SAME generated `database.rs`.  A `ForgeDb` #[napi] class whose methods
    // mirror the generated CRUD 1:1, calling the integrity wrappers + storage reads
    // by name.  Node and Bun share ONE `.node` (Option A).  Rows marshal natively
    // through the struct's own serde via Env::to_js_value — no generic query
    // builder, no `match model`.
    let src = r#"
User {
  id: +uuid
  email: &string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  view_count: u64
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  label: string
  posts: [Post]
}

Reading {
  id: +u64
  value: i64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = NapiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The `ForgeDb` #[napi] class + its schema-invariant lifecycle. Node and Bun
    // both load this one addon (Option A).
    assert!(
        flat.contains("#[napi(js_name=\"ForgeDb\")]"),
        "the ForgeDb #[napi] class exists"
    );
    assert!(flat.contains("structForgeDb"), "the ForgeDb handle exists");
    assert!(flat.contains("Database::open_at("), "open wraps Database::open_at");
    assert!(flat.contains("self.write().commit()"), "commit wraps the generated commit under the write guard");
    // The factory constructor + method annotations are emitted.
    assert!(flat.contains("#[napi(factory)]"), "open is a #[napi] factory");

    // The engine is shared behind an `Arc<RwLock<Database>>` so async ops can run
    // on a libuv pool thread over the SAME handle; reads take a shared guard, writes
    // the exclusive guard, both poison-recovering.
    assert!(flat.contains("inner:Arc<RwLock<Database>>"), "engine is shared behind Arc<RwLock>");
    assert!(
        flat.contains("fnread(&self)->std::sync::RwLockReadGuard")
            && flat.contains("fnwrite(&self)->std::sync::RwLockWriteGuard"),
        "poison-recovering read()/write() guard helpers are generated"
    );
    assert!(
        flat.contains("unwrap_or_else(|e|e.into_inner())"),
        "guards recover from a poisoned lock"
    );

    // Per-model CRUD, each keyed by the model's snake name, going through the
    // generated integrity wrappers + storage reads (not a reimplementation).
    for m in ["user", "post", "reading"] {
        for op in ["create", "get", "all", "count", "update", "delete"] {
            assert!(
                flat.contains(&format!("fn{op}_{m}(")),
                "missing per-model method {op}_{m}"
            );
        }
    }
    assert!(flat.contains("self.write().create_user("), "create uses the create_<m> integrity wrapper under the write guard");
    assert!(flat.contains("self.write().update_post("), "update uses the update_<m> wrapper under the write guard");
    assert!(flat.contains("self.write().delete_user("), "delete uses the delete_<m> referential wrapper under the write guard");
    assert!(flat.contains("self.read().user.get("), "get uses the generated storage read under the read guard");
    assert!(flat.contains("self.read().reading.row_count("), "count uses the generated row_count under the read guard");
    assert!(flat.contains("self.read().post.all()"), "all uses the generated storage read under the read guard");

    // Async (Promise-returning) CRUD variants — return `AsyncTask<AsyncOp>` (a JS
    // Promise) and run the engine call on a libuv pool thread under the shared lock.
    assert!(
        flat.contains("implTaskforAsyncOp") && flat.contains("typeJsValue=JsUnknown"),
        "the generic AsyncOp implements napi::Task"
    );
    for m in ["user", "post", "reading"] {
        for op in ["create", "get", "all", "update", "delete"] {
            assert!(
                flat.contains(&format!("fn{op}_{m}_async(")),
                "missing async variant {op}_{m}_async"
            );
        }
    }
    assert!(flat.contains("commit_async(&self)->AsyncTask<AsyncOp>"), "schema-wide commit_async exists");
    assert!(flat.contains("->Result<AsyncTask<AsyncOp>>"), "async ops return an AsyncTask (a JS Promise)");
    assert!(
        flat.contains("inner.write().unwrap_or_else(|e|e.into_inner())")
            && flat.contains("inner.read().unwrap_or_else(|e|e.into_inner())"),
        "async ops lock the shared handle on the pool thread",
    );

    // Typed row structs (Phase 5b/6b follow-up) — one `#[napi(object)]` struct per
    // identity model so napi-rs auto-emits a typed TypeScript interface in the
    // `.d.ts`. Struct fields match the serde JSON wire shape (uuid/decimal/enum →
    // String, u64 → i64, timestamp → i64, json → serde_json::Value, etc.).
    // (flat has all whitespace stripped, so `pub struct NapiUser` → `pubstructNapiUser`)
    for model in ["User", "Post", "Tag", "Reading"] {
        assert!(
            flat.contains(&format!("pubstructNapi{model}")),
            "typed #[napi(object)] row struct Napi{model} is generated"
        );
        assert!(
            flat.contains(&format!("#[napi(object,js_name=\"{model}\")]")),
            "Napi{model} is annotated #[napi(object, js_name)]"
        );
        assert!(
            flat.contains(&format!("Napi{model}::from_record")),
            "Napi{model}::from_record converter is present"
        );
    }
    // Typed fields in the row struct: User has `id: String` (uuid → String) and
    // `email: String`; Reading has `value: i64` (i64 field).
    // Note: u64 PK on Reading maps to i64 in the napi struct.
    assert!(flat.contains("pubid:String"), "User id field is typed String (uuid)");
    assert!(flat.contains("pubvalue:i64"), "Reading value field is typed i64");
    // Every row-struct field pins its JS key to the exact `.forge` field name via
    // `#[napi(js_name = ...)]`. Without this, `#[napi(object)]` camelCases multi-word
    // fields (`view_count` -> `viewCount`), diverging from the snake_case serde wire
    // shape used by create/update input, REST, and the TS SDK. The multi-word
    // `Post.view_count` is the regression guard.
    assert!(
        flat.contains("#[napi(js_name=\"view_count\")]"),
        "row-struct fields pin their snake_case JS key (Post.view_count stays snake_case, not viewCount)"
    );

    // get/all now return the typed napi struct instead of JsUnknown.
    assert!(
        flat.contains("->Result<Option<NapiUser>>"),
        "get_user returns typed Option<NapiUser>"
    );
    assert!(
        flat.contains("->Result<Vec<NapiPost>>") || flat.contains("->Result<Vec<NapiUser>>"),
        "all_<m> returns typed Vec<Napi<Model>>"
    );

    // Rows/ids marshal through the generated struct's serde via NAPI-RS's
    // serde bridge (native JS objects, one source of truth — no second per-field
    // matrix). Input (create/update) still uses the serde bridge; output uses
    // the typed Napi<Model> struct.
    assert!(flat.contains("database::User"), "rows decode into the generated User struct");
    assert!(flat.contains("env.from_js_value"), "inbound rows/ids via Env::from_js_value");
    assert!(flat.contains("env.to_js_value"), "outbound ids (create) via Env::to_js_value");

    // Integrity failures throw a JS Error; engine calls are catch_unwind-guarded
    // (a panic must not unwind across the Node-API boundary).
    assert!(flat.contains("Error::from_reason"), "errors surface as a thrown JS Error");
    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

    // Relation traversal (Phase 6b) — forward FK / reverse 1:M / M2M, mirroring the
    // generated `Database` getters by name. Row-returning methods return typed
    // Napi<Model> structs (typed ergonomics follow-up).
    assert!(
        flat.contains("pubfnpost_author(&self,env:Env,id:JsUnknown)->Result<Option<NapiUser>>"),
        "forward FK post_author returns typed Option<NapiUser>"
    );
    assert!(
        flat.contains("let__db=self.read();__db.post.get(id).and_then(|__rec|__db.post_author(&__rec))"),
        "forward FK fetches the source then resolves the generated getter"
    );
    assert!(
        flat.contains("pubfnuser_posts(&self,env:Env,id:JsUnknown)->Result<Vec<NapiPost>>"),
        "reverse 1:M user_posts returns typed Vec<NapiPost>"
    );
    assert!(flat.contains("pubfnlink_post_tag(&self"), "M2M link_post_tag exists");
    assert!(
        flat.contains("pubfnunlink_post_tag(&self") && flat.contains("->Result<bool>"),
        "M2M unlink_post_tag returns bool"
    );
    assert!(
        flat.contains("pubfnpost_tags(&self,env:Env,id:JsUnknown)->Result<Vec<NapiTag>>"),
        "M2M forward query post_tags returns typed Vec<NapiTag>"
    );
    assert!(
        flat.contains("pubfntag_posts(&self,env:Env,id:JsUnknown)->Result<Vec<NapiPost>>"),
        "M2M reverse query tag_posts returns typed Vec<NapiPost>"
    );

    // Arrow columnar export (Phase 6b) — per-exportable-column methods returning a
    // zero-copy external ArrayBuffer + format + length (unchanged from prior phase).
    assert!(
        flat.contains("pubfnreading_value_arrow(&self,env:Env)->Result<JsUnknown>"),
        "the exportable i64 column gets a zero-copy Arrow method"
    );
    assert!(flat.contains("pubfnreading_id_arrow(&self,env:Env)"), "the u64 PK column is exportable");
    assert!(
        flat.contains("create_arraybuffer_with_borrowed_data"),
        "the export aliases the column bytes into an external ArrayBuffer (zero-copy)"
    );
    assert!(
        flat.contains("__db.reading.export_live_indices()") && flat.contains(".export_col_value("),
        "the export computes the live set in generated code + gathers the one column"
    );
    assert!(flat.contains("set_named_property(\"format\""), "the result carries the Arrow format string");
    // A non-exportable column (variable-length string) gets NO Arrow method.
    assert!(!flat.contains("post_title_arrow"), "a string column is not Arrow-exportable");

    // IDENTITY: no generic query surface / runtime schema dispatch.
    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "the NAPI-RS binding must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_api_generation_replication_endpoint() {
    // The generated replication WS endpoint (#82 Direction C): one schema-wide
    // /replicate route behind the tenant-auth guard, a resumable handshake
    // (?after=<offset> → durable replay + live tail), and FIELD-BLIND binary
    // frames (PersistedEvent::to_wire) — the handler forwards opaque bytes and
    // never decodes a frame.
    let src = r#"
Post {
  id: +uuid
  title: string
  author: *User
}

User {
  id: +uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;
    let norm = code.replace(" :: ", "::").replace(" . ", ".");

    // A single schema-wide handler + route (NOT per model).
    assert!(
        code.contains("async fn __replicate") && code.contains("async fn __handle_replicate"),
        "the replication upgrade + handler are generated"
    );
    assert!(
        norm.contains(".route(\"/replicate\", get(__replicate))")
            || code.contains("/replicate"),
        "the /replicate route is registered"
    );

    // Resumable handshake: reads ?after=<offset>, replays durably, streams live.
    assert!(
        code.contains("\"after\""),
        "the handler resumes from the ?after=<offset> query param"
    );
    assert!(
        norm.contains("catch_up_from(after, usize::MAX)"),
        "the handler uses the broker's race-free catch_up_from"
    );
    // Idempotent by absolute offset: skip anything the replay already covered.
    assert!(
        code.contains("ev.offset <= boundary"),
        "the live tail is idempotent by absolute offset (skip <= boundary)"
    );

    // Field-blind binary frames: opaque wire bytes, never a decoded field.
    assert!(
        norm.contains("ev.to_wire()"),
        "frames are sent as opaque binary wire bytes"
    );
    assert!(
        code.contains("db.read().await.broker.clone()")
            || norm.contains("db.read().await.broker.clone()"),
        "the handler reads the shared broker from the Database"
    );

    // The route sits INSIDE __data_routes (behind the tenant-auth guard).
    let data_routes = code
        .split("fn __data_routes")
        .nth(1)
        .and_then(|s| s.split("fn __ops_routes").next())
        .unwrap_or("");
    assert!(
        data_routes.contains("/replicate"),
        "the /replicate route must be behind the tenant-auth guard (in __data_routes)"
    );

    // Identity: no model-name field-decoding branch in the transport.
    assert!(
        !code.contains("match model_name"),
        "the replication transport must not decode a field by model name"
    );
}

#[test]
fn test_api_generation_websocket_subscription() {
    // The generated WS endpoint (#62 Direction A): a per-model /subscribe route,
    // an upgrade handler that routes by model NAME and materializes a typed event
    // from the row index, and a GENERATED per-model filter (field-by-field, so the
    // substrate never inspects a field).
    let src = r#"
Post {
  id: +uuid
  title: string
  views: u64
  author: *User
}

User {
  id: +uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // WS imports + per-model subscription route.
    assert!(code.contains("WebSocketUpgrade"), "ws upgrade imported");
    assert!(
        code.contains("async fn subscribe_post"),
        "per-model subscription handler generated"
    );
    assert!(
        code.contains("/subscribe/"),
        "per-model /subscribe route registered"
    );

    // Model routing is by NAME; the handler now streams Inserted/Updated/Deleted
    // typed events (#66), skipping only M2M Linked signals.
    assert!(
        code.contains("event.model != \"Post\""),
        "handler routes by model name"
    );
    assert!(
        code.contains("PostInserted") && code.contains("PostUpdated") && code.contains("PostDeleted"),
        "handler streams typed Inserted/Updated/Deleted events (#66)"
    );
    assert!(
        code.contains("ChangeKind::Linked => continue") || code.contains("ChangeKind :: Linked => continue"),
        "handler skips M2M Linked signals for a model subscription"
    );
    // Materialize via the public read_at using the broadcast row index.
    assert!(
        code.contains(".post.read_at(event.row_index)"),
        "handler materializes the typed record from the row index"
    );

    // Generated per-model filter names each declared scalar field explicitly —
    // the relation field (`author`) is NOT filterable and must be absent.
    assert!(
        code.contains("fn post_event_matches"),
        "generated per-model filter"
    );
    assert!(code.contains("params.get(\"title\")"), "scalar field is filterable");
    assert!(code.contains("params.get(\"views\")"), "integer field is filterable");
    assert!(
        !code.contains("params.get(\"author\")"),
        "relation field must not be a filter key"
    );
}

#[test]
fn test_rust_generation_reader_handles() {
    // #56 Direction B: read-only reader handles for single-writer/many-reader.
    // Each model gets a `*StorageReader` (shared-fd column readers) with the SAME
    // read_at/get_at/all_at surface; the junction gets a `*Reader` with pairs_at;
    // the Database gets a `DatabaseReader` bundle (one typed reader field per model
    // AND junction — never a string-keyed dispatch) + `reader()`, plus the
    // snapshot-scoped M2M traversal on the reader.
    let src = r#"
Post {
  id: +uuid
  title: string
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Per-model reader struct + shared-fd column reader types.
    assert!(code.contains("pub struct PostStorageReader"), "per-model reader struct");
    assert!(
        code.contains("forgedb_storage::FixedColumnReader")
            || code.contains("forgedb_storage :: FixedColumnReader"),
        "reader uses the substrate FixedColumnReader (shared fd)"
    );
    assert!(
        code.contains("forgedb_storage::VariableColumnReader")
            || code.contains("forgedb_storage :: VariableColumnReader"),
        "reader uses the substrate VariableColumnReader"
    );
    assert!(
        code.contains("forgedb_storage::TombstonesReader")
            || code.contains("forgedb_storage :: TombstonesReader"),
        "reader uses the substrate TombstonesReader"
    );
    // `*Storage::reader()` opens the shared-fd handle.
    assert!(
        code.contains("pub fn reader(&self) -> PostStorageReader"),
        "storage exposes a reader() handle"
    );
    assert!(
        code.contains(".reader().expect(\"Failed to open column reader\")"),
        "reader shares the writer's column fds via col.reader()"
    );

    // The reader reuses the SAME tailored read surface (no second decode path).
    assert!(
        code.contains("impl PostStorageReader"),
        "reader impl block generated"
    );
    // read_at + get_at + all_at appear for BOTH the writer storage and the reader
    // (i.e. at least twice each in the emitted code).
    assert!(
        code.matches("fn read_at(&self, row_index: usize) -> Option<Post>").count() >= 2,
        "read_at is emitted for both the writer storage and the reader"
    );
    assert!(
        code.matches("pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Post>").count()
            + code.matches("pub fn all_at(&self, snap: &forgedb_storage :: Snapshot) -> Vec<Post>").count()
            >= 2,
        "all_at is emitted for both the writer storage and the reader"
    );

    // Junction reader with a watermark-clamped pairs_at.
    assert!(code.contains("pub struct PostTagLinkReader"), "junction reader struct");
    assert!(
        code.contains("pub fn reader(&self) -> PostTagLinkReader"),
        "junction exposes a reader() handle"
    );

    // DatabaseReader bundle: one typed reader field per model AND junction — the
    // red line is that these are NAMED generated fields, not a runtime string map.
    assert!(code.contains("pub struct DatabaseReader"), "DatabaseReader bundle");
    assert!(code.contains("pub post: PostStorageReader"), "typed per-model reader field");
    assert!(code.contains("pub tag: TagStorageReader"), "typed per-model reader field");
    assert!(
        code.contains("pub post_tag_link: PostTagLinkReader"),
        "typed per-junction reader field"
    );
    assert!(
        code.contains("pub fn reader(&self) -> DatabaseReader"),
        "Database::reader() opens the whole-db read handle"
    );
    // No string-keyed generic read dispatch anywhere (identity red line).
    assert!(
        !code.contains("fn read_at(&self, model: &str") && !code.contains("model_name: &str"),
        "no runtime model-name-keyed read dispatch"
    );

    // Snapshot-scoped M2M traversal is generated on the reader too.
    assert!(
        code.contains("impl DatabaseReader"),
        "reader-side traversal impl block generated"
    );
    assert!(
        code.matches("pub fn post_tags_at(&self, snap: &DatabaseSnapshot, id: Uuid) -> Vec<Tag>").count()
            >= 2,
        "post_tags_at is generated on both Database and DatabaseReader"
    );
}

#[test]
fn test_rust_generation_live_delta_enums() {
    // #62 Direction B: per-model typed live-query delta enum (Init/Added/Updated/
    // Removed), tagged JSON, over generated model records + the model's id type.
    let src = r#"
Post {
  id: +uuid
  title: string
}

Counter {
  id: +u64
  label: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub enum PostLiveDelta"), "per-model live-delta enum");
    assert!(
        code.contains("#[serde(tag = \"kind\", rename_all = \"lowercase\")]"),
        "tagged JSON so clients dispatch on kind"
    );
    assert!(code.contains("Init { rows: Vec<Post> }"), "Init carries the full set");
    assert!(code.contains("Added { row: Post }"), "Added carries a typed record");
    assert!(code.contains("Updated { row: Post }"), "Updated carries a typed record");
    assert!(code.contains("Removed { id: Uuid }"), "Removed carries the uuid id");
    // Integer-PK model's Removed carries the integer id type.
    assert!(
        code.contains("pub enum CounterLiveDelta") && code.contains("Removed { id: u64 }"),
        "integer-PK Removed carries the u64 id"
    );
}

#[test]
fn test_api_generation_live_query() {
    // #62 Direction B: the live-query WS handler. THE red line (drift vector #2):
    // the `?field=value` binding must reuse the SAME generated closed-set filter
    // (`<model>_event_matches`) as REST list / #62-A — no second predicate parser.
    // Re-evaluation runs only the GENERATED query (all() + that filter). The feed
    // is consulted COARSELY (only event.model). Deltas are the generated typed
    // enum; membership is opaque id -> opaque hash.
    let src = r#"
Post {
  id: +uuid
  title: string
  author: *User
}

User {
  id: +uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // Route + handler.
    assert!(code.contains("/live-query/"), "per-model /live-query route registered");
    assert!(
        code.contains("async fn subscribe_live_post"),
        "per-model live-query handler generated"
    );

    // RED LINE #1: binds to the SAME generated closed-set filter — no second parser.
    assert!(
        code.contains("post_event_matches(r, &params)"),
        "live-query reuses the generated closed-set filter (no second predicate parser)"
    );
    // Re-evaluation runs the GENERATED query (all() + the generated filter).
    assert!(
        code.contains(".post.all()") || code.contains(".post\n"),
        "live-query re-runs the generated all() query"
    );
    // No runtime-interpreted query/predicate string parser.
    assert!(
        !code.contains("parse_predicate") && !code.contains("__gt") && !code.contains("__like"),
        "no operator grammar / predicate-as-data parser"
    );

    // COARSE signal: only event.model is consulted (never row_index/kind) in the
    // live-query path — so no logical-row identity is resolved via the substrate.
    assert!(
        code.contains("if event.model != \"Post\""),
        "live-query re-runs on the coarse model signal"
    );

    // Typed deltas + TYPED membership (#84: no opaque stringify hash).
    assert!(
        code.contains("PostLiveDelta::Init") || code.contains("PostLiveDelta :: Init"),
        "handler streams the generated typed delta enum"
    );
    assert!(
        code.contains("PostLiveDelta::Removed") || code.contains("PostLiveDelta :: Removed"),
        "handler emits removal-aware deltas"
    );
    // #84: membership stores the TYPED record and change-detection uses the
    // generated typed comparator — not a `serde_json` stringify hash.
    assert!(
        code.contains("HashMap<Uuid, super::Post>"),
        "membership tracks id -> typed record (not an opaque string hash)"
    );
    assert!(
        code.contains("post_record_changed(prev, &r)"),
        "live-query Updated diff uses the typed per-field comparator (#84)"
    );
    assert!(
        !code.contains("serde_json::to_string(&r).unwrap_or_default()")
            && !code.contains("serde_json :: to_string(& r)"),
        "no whole-record stringify hashing remains in the diff (#84)"
    );
    // The relation field stays non-filterable (inherited from the shared filter).
    assert!(
        !code.contains("params.get(\"author\")"),
        "relation field is not a live-query filter key"
    );
}

#[test]
fn test_api_generation_typed_event_filter() {
    // #84: the generated `<model>_event_matches` filter must compare TYPED values
    // (parse the string param into the field's Rust type, then `==`), not the old
    // `serde_json` stringify — so `?price=3` matches a stored `3.0`, and
    // bool/enum/decimal/timestamp compare by value. And the live-query `Updated`
    // diff must use a generated typed per-field change detector.
    let src = r#"
enum Status {
  Active,
  Pending,
  Closed,
}

Widget {
  id: +uuid
  name: string
  price: f64
  discount: f64?
  in_stock: bool
  status: Status
  cost: decimal
  made_at: timestamp
  quantity: i32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    // TYPED filter parses per field type (#84).
    assert!(
        code.contains("want.parse::<f64>()"),
        "f64 filter parses the param to f64 (so ?price=3 matches 3.0)"
    );
    assert!(
        code.contains("want.parse::<bool>()"),
        "bool filter parses the param to bool"
    );
    assert!(
        code.contains("want.parse::<rust_decimal::Decimal>()"),
        "decimal filter parses the param to Decimal"
    );
    assert!(
        code.contains("want.parse::<i32>()"),
        "i32 filter parses the param to i32"
    );
    assert!(
        code.contains("want.parse::<forgedb_types::Timestamp>()"),
        "#254: a timestamp filter parses the RFC 3339 wire form, the same form the \
         body uses — parsing a bare integer here would have silently meant SECONDS \
         against microsecond storage, matching nothing instead of failing"
    );
    // (The generic path may wrap across lines in the formatted output, so match
    // the pieces rather than one contiguous span.)
    assert!(
        code.contains("serde_json::from_value::<") && code.contains("super::Status"),
        "enum filter reuses the canonical variant-name serde mapping"
    );

    // The OLD fragile stringify filter body is gone (#84).
    assert!(
        !code.contains("serde_json::to_value(record)")
            && !code.contains("other.to_string() == *want"),
        "no stringify-compare remains in the event filter"
    );

    // Typed per-field change detector for the live-query diff (#84): f64 by bits.
    assert!(
        code.contains("fn widget_record_changed"),
        "generated typed change detector"
    );
    assert!(
        code.contains(".to_bits() != ") ,
        "f64 change-detection compares bit patterns (deterministic, NaN-stable)"
    );

    // The filterable set is unchanged — a relation-less scalar model still filters
    // by every declared scalar; the id/enum keys are present.
    assert!(
        code.contains("params.get(\"status\")"),
        "enum field is a filter key"
    );
}

// ---- OpenAPI generation (#49) ----------------------------------------------

#[test]
fn test_openapi_generation_multiple_models() {
    let schema = multi_model_schema();
    let result = OpenApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_openapi_generation_fk_schema() {
    let schema = fk_schema();
    let result = OpenApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

/// The emitted spec must be a well-formed OpenAPI 3.1 document (this is the
/// analogue of the compile-test discipline for a non-Rust artifact: parse the
/// output back and assert structure, rather than only snapshotting the string).
#[test]
fn test_openapi_generation_is_valid_document() {
    let schema = fk_schema();
    let code = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&code).expect("output is valid JSON");

    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["info"]["title"].is_string());
    assert!(spec["servers"].is_array());

    // Routes mirror the generated API: kebab path, list/create + get/put/delete.
    let paths = &spec["paths"];
    assert!(paths["/api/post"]["get"].is_object(), "list route");
    assert!(paths["/api/post"]["post"].is_object(), "create route");
    let item = &paths["/api/post/{id}"];
    assert!(item["get"].is_object(), "get-by-id route");
    assert!(item["put"].is_object(), "replace route");
    assert!(item["delete"].is_object(), "delete route");
    // The {id} path parameter is declared.
    assert_eq!(item["parameters"][0]["name"], "id");
    assert_eq!(item["parameters"][0]["in"], "path");

    // Every $ref resolves into components/schemas.
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("components.schemas is an object");
    assert!(schemas.contains_key("Author"));
    assert!(schemas.contains_key("Post"));

    // FK scalars are documented; the required FK is non-nullable, the optional
    // one is nullable via a 3.1 `["string", "null"]` type union (no `nullable`
    // keyword in 3.1).
    let post = &schemas["Post"];
    assert_eq!(post["properties"]["author_id"]["type"], "string");
    assert_eq!(post["properties"]["author_id"]["format"], "uuid");
    assert!(post["properties"]["editor_id"].get("nullable").is_none());
    let editor_type = post["properties"]["editor_id"]["type"]
        .as_array()
        .expect("optional FK uses a type union");
    assert!(editor_type.iter().any(|v| v == "string"));
    assert!(editor_type.iter().any(|v| v == "null"));
    let required: Vec<&str> = post["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"author_id"), "required FK is required");
    assert!(!required.contains(&"editor_id"), "optional FK is not required");
}

/// Virtual collection fields (one-to-many / many-to-many) and component
/// references carry no body value and must be omitted from the schema.
#[test]
fn test_openapi_generation_skips_virtual_fields() {
    let schema = component_schema();
    let code = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&code).unwrap();

    // component_schema()'s model has a OneToMany + Component field; neither is a
    // serialized scalar, so neither appears as a property.
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for (_name, model) in schemas {
        if let Some(props) = model["properties"].as_object() {
            for (_field, prop) in props {
                // No property should be an empty/null schema — every emitted
                // property has a concrete `type` or `$ref`.
                assert!(
                    prop.get("type").is_some()
                        || prop.get("$ref").is_some()
                        || prop.get("anyOf").is_some(),
                    "every property has a concrete schema"
                );
            }
        }
    }
}

#[test]
fn test_rust_generation_decimal_type() {
    // `decimal` is an exact fixed-point value (rust_decimal::Decimal) on the
    // FIXED 16-byte column path (like uuid): Decimal::serialize()/deserialize().
    // It is filterable/sortable/indexable (Ord+Hash), with a SCALE-INVARIANT
    // index key (value.normalize()) so `1.0` and `1.00` share one bucket.
    let src = r#"
Product {
  id: +uuid
  price: ^decimal
  discount: decimal?
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // 1. Struct field is rust_decimal::Decimal (non-null) / Option<...> (nullable),
    //    serialized as a JSON string (precision-preserving) via the serde `str`
    //    module, and schema'd as a String (Decimal has no ToSchema impl).
    assert!(
        code.contains("pub price: rust_decimal::Decimal"),
        "non-null decimal field type"
    );
    assert!(
        code.contains("pub discount: Option<rust_decimal::Decimal>"),
        "nullable decimal field type"
    );
    assert!(
        code.contains("#[serde(with = \"rust_decimal::serde::str\")]"),
        "non-null decimal uses the string serde module"
    );
    assert!(
        code.contains("#[serde(with = \"rust_decimal::serde::str_option\")]"),
        "nullable decimal uses the string_option serde module"
    );
    assert!(
        code.contains("#[schema(value_type = String)]"),
        "decimal is schema'd as a String for utoipa"
    );

    // 2. Fixed 16-byte column storage — the raw uuid byte path but with a
    //    Decimal::serialize()/deserialize() round-trip (not uuid's as_bytes).
    assert!(
        code.contains(".append_uuid(record.price.serialize())"),
        "non-null decimal appends via Decimal::serialize() -> [u8; 16]"
    );
    assert!(
        code.contains("rust_decimal::Decimal::deserialize(bytes)"),
        "non-null decimal reads via Decimal::deserialize([u8; 16])"
    );
    // The column is a FixedColumn (not a VariableColumn like string/json).
    assert!(
        code.contains("price_col: FixedColumn"),
        "decimal occupies a fixed column"
    );
    // Nullable decimal rides the generic nullable-fixed-byte path (Option<Decimal>).
    assert!(
        code.contains("std::mem::size_of::<Option<rust_decimal::Decimal>>()"),
        "nullable decimal sizes as Option<Decimal>"
    );

    // 3. Scale-invariant index key: an explicitly-indexed decimal field
    //    (`^decimal`) is indexed and its value is normalized before keying.
    assert!(code.contains("price_index"), "^decimal is indexed");
    assert!(
        code.contains("(record.price).normalize()"),
        "the decimal index key normalizes away scale (1.0 == 1.00)"
    );

    // 4. Sort uses Ord::cmp (decimal is Ord) — never the float partial_cmp branch.
    let api = ApiGenerator::generate(&schema).unwrap().code;
    assert!(
        api.contains("\"price\" => rows.sort_by(|a, b| a.price.cmp(&b.price))"),
        "decimal sorts via Ord::cmp, not partial_cmp"
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_enum_type() {
    // A user-declared `enum` (#enum) is a NEW top-level declaration referenced by
    // its bare PascalCase name.  It becomes a fieldless Rust enum (serialized as
    // its variant NAME string) stored in a FIXED 1-byte u8 discriminant column
    // (2 bytes for nullable — `[present, disc]`), mapped via a generated
    // `__to_u8`/`__from_u8` codec.  Ord+Hash ⇒ filterable/sortable/indexable.
    let src = r#"
enum OrderStatus { Pending, Paid, Shipped }

Order {
  id: +uuid
  label: string
  status: ^OrderStatus
  prev_status: OrderStatus?
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();

    // The parser resolved the bare identifier to `FieldType::Enum` (not a struct).
    let order = schema.models.iter().find(|m| m.name == "Order").unwrap();
    let status = order.fields.iter().find(|f| f.name == "status").unwrap();
    assert_eq!(status.field_type, FieldType::Enum("OrderStatus".to_string()));
    let prev = order.fields.iter().find(|f| f.name == "prev_status").unwrap();
    assert_eq!(
        prev.field_type,
        FieldType::Nullable(Box::new(FieldType::Enum("OrderStatus".to_string())))
    );

    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    // 1. The fieldless enum definition with the expected derives + variants.
    assert!(code.contains("pub enum OrderStatus"), "enum def emitted");
    assert!(
        code.contains("Copy") && code.contains("Ord") && code.contains("Hash"),
        "enum derives include Copy/Ord/Hash (needed for index + sort)"
    );

    // 2. The 1-byte discriminant codec (declaration-order 0..N) + out-of-range guard.
    assert!(
        code.contains("fn __to_u8(&self) -> u8"),
        "enum has a __to_u8 discriminant encoder"
    );
    assert!(
        code.contains("OrderStatus::Pending => 0u8")
            && code.contains("OrderStatus::Paid => 1u8")
            && code.contains("OrderStatus::Shipped => 2u8"),
        "variants map to 0..N in declaration order"
    );
    assert!(
        code.contains("fn __from_u8(__b: u8) -> OrderStatus"),
        "enum has a __from_u8 decoder"
    );
    assert!(
        code.contains("discriminant byte"),
        "__from_u8 hard-fails on an out-of-range byte"
    );

    // 3. Fixed 1-byte column storage — an explicit append_bytes([disc]) / read_bytes
    //    path (NOT the string/variable path, NOT read_unaligned).
    assert!(
        code.contains("status_col: FixedColumn"),
        "enum occupies a fixed column"
    );
    assert!(
        code.contains(".append_bytes(&[record.status.__to_u8()])"),
        "non-null enum writes its 1-byte discriminant"
    );
    assert!(
        code.contains("OrderStatus::__from_u8(bytes[0])"),
        "non-null enum reads its discriminant back via __from_u8"
    );
    // Nullable enum: a 2-byte [present, disc] column, distinguishing None from
    // Some(variant-0).
    assert!(
        code.contains("Some(v) => [1u8, v.__to_u8()]"),
        "nullable enum encodes a presence tag + discriminant"
    );
    assert!(
        code.contains("Some(OrderStatus::__from_u8(bytes[1]))"),
        "nullable enum decodes present values via __from_u8"
    );

    // 4. Indexing: `^OrderStatus` is a secondary index keyed by the variant NAME
    //    string (the default serde form) via the shared index_key_expr path.
    assert!(code.contains("status_index"), "^enum field is indexed");
    assert!(
        code.contains("pub fn find_by_status(&self, value: OrderStatus)"),
        "enum probe takes the enum type"
    );

    // 5. Sort uses Ord::cmp (enum is Ord), never the float partial_cmp branch.
    let api = ApiGenerator::generate(&schema).unwrap().code;
    assert!(
        api.contains("\"status\" => rows.sort_by(|a, b| a.status.cmp(&b.status))"),
        "enum sorts via Ord::cmp"
    );

    // 6. TypeScript: a closed string-union type alias + the field typed as it.
    let ts = TypeScriptGenerator::generate(&schema).unwrap().code;
    assert!(
        ts.contains("export type OrderStatus = \"Pending\" | \"Paid\" | \"Shipped\";"),
        "TS emits a string-union alias for the enum"
    );
    assert!(
        ts.contains("status: OrderStatus;"),
        "TS field is typed as the enum union"
    );

    // 7. OpenAPI: a `{ type: string, enum: [...] }` component schema referenced
    //    by the field.
    let oa = OpenApiGenerator::generate(&schema).unwrap().code;
    let oa_json: serde_json::Value = serde_json::from_str(&oa).unwrap();
    let enum_schema = &oa_json["components"]["schemas"]["OrderStatus"];
    assert_eq!(enum_schema["type"], "string", "OpenAPI enum is a string");
    assert_eq!(
        enum_schema["enum"],
        serde_json::json!(["Pending", "Paid", "Shipped"]),
        "OpenAPI enum lists the variant names"
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_transaction() {
    // MVCC Tier 1 (#83, M1a): the generated `Database` gains a transaction API — a
    // `TxHandle` with scoped per-model `create_/update_/delete_` writes + `get_/all_`
    // reads, `Database::transaction(|tx| ...)`, staged-append (no eager index/feed),
    // read-your-writes via a raised watermark, commit = advance visibility, rollback
    // = truncate to marks.  Entirely generated on the tailored Database — identity
    // red line: no generic runtime transaction executor.
    let src = r#"
User {
  id: +uuid
  email: &string
  age: ^u32
}

Post {
  id: +uuid
  author: *User
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // The transaction entry point + handle are generated on the tailored Database.
    assert!(
        code.contains("pub fn transaction<T>(")
            && code.contains("impl FnOnce(&mut TxHandle) -> Result<T, TxError>"),
        "Database::transaction takes a closure over a scoped TxHandle"
    );
    assert!(
        code.contains("pub struct TxHandle<'db>"),
        "generated per-schema TxHandle struct"
    );
    assert!(
        code.contains("pub enum TxError"),
        "TxError type is generated"
    );
    assert!(
        code.contains("impl From<ValidationError> for TxError"),
        "a `?` on a staged validated write propagates a #91 ValidationError"
    );

    // Scoped per-model write + read methods exist on the handle.
    assert!(
        code.contains("pub fn create_user(&mut self, mut record: User) -> Result<Uuid, TxError>")
            && code.contains("pub fn update_user(")
            && code.contains("pub fn delete_user("),
        "TxHandle exposes scoped create/update/delete per model"
    );
    assert!(
        code.contains("pub fn get_user(&self, id: Uuid) -> Option<User>")
            && code.contains("pub fn all_user(&self) -> Vec<User>"),
        "TxHandle exposes scoped get/all per model"
    );

    // Read-your-writes: reads go through `get_at`/`all_at` with the watermark raised
    // to the current staged physical length — ONE decode path, not a forked reader.
    assert!(
        code.contains("forgedb_storage::Snapshot::new(self.db.user.row_count)")
            && code.contains("self.db.user.get_at(&__snap, id)"),
        "txn reads resolve via get_at at the raised (staged) watermark"
    );

    // Staged writes use the low-level __stage_append (WAL + columns only — NO
    // id_to_row/index/feed/broker), so rollback is a pure truncate.
    assert!(
        code.contains("pub fn __stage_append(&mut self, record: User, deleted: bool) -> usize"),
        "generated low-level staged-append that skips index/feed/broker"
    );
    assert!(
        code.contains("self.db.user.__stage_append(record, false)"),
        "TxHandle::create stages via __stage_append"
    );

    // Rollback = truncate every touched collection's columns + WAL tail to the mark.
    assert!(
        code.contains("fn rollback_internal(&mut self)")
            && code.contains("__truncate_all_to(__mark)")
            && code.contains("wal.truncate_to("),
        "rollback truncates staged rows + the staged WAL tail back to the mark"
    );
    // The Drop backstop rolls back an un-committed handle (panic safety).
    assert!(
        code.contains("impl<'db> Drop for TxHandle<'db>")
            && code.contains("self.rollback_internal();"),
        "an un-committed TxHandle rolls back on drop"
    );

    // Commit advances visibility ONLY on the Ok path (rebuild indexes) — never a
    // mid-txn eager index write.  The buffered events drain to the broker on commit.
    assert!(
        code.contains("self.db.user.__reindex_committed();"),
        "commit advances visibility by rebuilding id_to_row + indexes"
    );
    assert!(
        code.contains("pending_events"),
        "changefeed/broker events are buffered and drained on commit"
    );

    // Intra-txn `&unique` duplicate guard (M1a fix): the `staged_unique_keys` buffer
    // catches two `create_<model>` calls with the same `&unique` value in ONE txn.
    // Keyed by (model, field, value) since #257 — see
    // `test_rust_generation_txn_unique_is_model_scoped`.
    assert!(
        code.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains(
                "staged_unique_keys:std::collections::BTreeSet<(&'staticstr,&'staticstr,String)>"
            ),
        "TxHandle carries a staged-unique-key buffer for intra-txn duplicate detection"
    );
    assert!(
        code.contains("self.staged_unique_keys.contains(") && code.contains("self.staged_unique_keys.insert("),
        "staged write checks then claims unique keys in the buffer"
    );

    // Identity: no generic runtime executor — dispatch is on the OPAQUE model tag,
    // never a decoded field (the same discipline as replication).
    assert!(
        !code.contains("match record.") && !code.contains("match field_name"),
        "transaction machinery must never match on a decoded field"
    );

    // #170 group commit: staged rows use the BUFFERED (no-fsync) WAL append, so a
    // transaction pays ONE barrier (the commit's `wal.flush()`) instead of one per
    // staged row. The committed insert/update/delete path keeps the per-op `write`.
    let stage_body = &code[code.find("fn __stage_append").unwrap()..];
    let stage_body = &stage_body[..stage_body.find("row_index\n").unwrap_or(stage_body.len().min(1500))];
    assert!(
        stage_body.contains(".write_buffered("),
        "#170: __stage_append uses the buffered (no-fsync) WAL append"
    );
    assert!(
        !stage_body.contains(".write(&forgedb_wal::WalEntry"),
        "#170: __stage_append does NOT per-record fsync (no plain wal.write)"
    );
    // The committed single-write path keeps the durable per-op fsync.
    let insert_body = &code[code.find("pub fn insert(").unwrap_or(0)..];
    assert!(
        insert_body.contains(".write(&forgedb_wal::WalEntry"),
        "#170: committed insert still fsyncs per op (durable single write unchanged)"
    );
}

#[test]
fn test_rust_generation_txn_intra_unique() {
    // MVCC Tier 1 (#83, M1a fix): two `create_<model>` calls with the same `&unique`
    // field value inside a single transaction must be caught and rejected — the
    // committed index cannot see staged rows, so a dedicated staged-unique buffer is
    // required.  The buffer is discarded on rollback without touching the real index.
    let src = r#"
User {
  id: +uuid
  email: &string
  username: &string
  age: u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // The buffer is declared on TxHandle and initialized empty.  The key is the
    // (model, field, value) triple — see `test_rust_generation_txn_unique_is_model_scoped`.
    let flat_decl: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat_decl
            .contains("staged_unique_keys:std::collections::BTreeSet<(&'staticstr,&'staticstr,String)>"),
        "TxHandle must carry a staged-unique buffer keyed by (model, field, value)"
    );
    assert!(
        code.contains("staged_unique_keys: std::collections::BTreeSet::new()"),
        "TxHandle::begin initializes the buffer empty"
    );

    // BOTH `&unique` fields (email + username) are guarded against intra-txn duplicates.
    // For each field the generated code checks `staged_unique_keys.contains(...)` then
    // inserts on success; a second staged write with the same value hits the contains
    // check and returns `Err(TxError::Validation(ValidationError::Unique{..}))`.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.matches("staged_unique_keys.contains(").count() >= 2,
        "both &unique fields (email, username) have a staged-buffer contains-check"
    );
    assert!(
        flat.matches("staged_unique_keys.insert(").count() >= 2,
        "both &unique fields claim their key in the buffer on success"
    );

    // The check fires BEFORE the stage append — a rejected duplicate leaves no staged row.
    let create_idx = code.find("pub fn create_user(").expect("create_user method");
    let create_body = &code[create_idx..];
    let contains_pos = create_body.find("staged_unique_keys.contains(").expect("contains check");
    let stage_pos = create_body.find("__stage_append").expect("stage append");
    assert!(
        contains_pos < stage_pos,
        "staged-unique check fires before the stage append (a rejected dup leaves no row)"
    );
}

#[test]
fn test_rust_generation_txn_unique_is_model_scoped() {
    // #257: a `&unique` claim staged inside a transaction is identified by
    // (model, field, value) — NOT by field name alone.  Keyed by field alone, two
    // unrelated models that merely share a field name landed in one uniqueness
    // namespace, and writing the same value to both in a single transaction was
    // rejected as a duplicate.  The staged set stands in for the committed index,
    // and that index is addressed per model AND per column, so the key must match.
    let src = r#"
Widget {
  id: +uuid
  code: &u64
  name: string
}

Gadget {
  id: +uuid
  code: &u64
  label: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Both models declare `code`; each must claim it under its OWN model tag.
    for model in ["Widget", "Gadget"] {
        let claim = format!("staged_unique_keys.insert(({model:?}, \"code\"");
        assert!(
            code.contains(&claim),
            "{model}.code must claim its staged key under its own model tag, not bare `code`"
        );
    }
    // The unqualified form must be gone everywhere — that IS the bug.
    assert!(
        !code.contains("staged_unique_keys.insert((\"code\","),
        "no staged claim may be keyed by field name alone (#257)"
    );
    assert!(
        !code.contains("staged_unique_keys.contains(&(\"code\","),
        "no staged lookup may be keyed by field name alone (#257)"
    );

    // The same qualification reaches the opaque MVCC write-set, at every tier.
    // Keys are length-framed so no component list can encode to another's bytes,
    // and tagged so a row key and a unique claim never share a space.
    assert!(
        code.contains("fn __forgedb_ws_key(parts: &[&[u8]]) -> Vec<u8>"),
        "conflict keys are built by the length-framing helper"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(
        flat.matches("&[b\"u\",__mtag.as_bytes(),__fname.as_bytes(),__ekey.as_bytes()")
            .count(),
        3,
        "all three write-set builders (TxHandle, Tier 2, Tier 3) carry the model tag"
    );
    assert_eq!(
        flat.matches("&[b\"r\",__model.as_bytes(),__id_bytes").count(),
        3,
        "all three write-set builders frame the row key the same way"
    );
    assert!(
        !flat.contains("k.extend_from_slice(__fname.as_bytes());"),
        "no write-set key may be a bare concatenation starting at the field name (#257)"
    );
}

/// Whether `code` mentions `ident` as a whole identifier rather than as a
/// substring of a longer one. `"ref_id_index".contains("id_index")` is true, so a
/// plain `contains` cannot assert that `id_index` was *not* generated.
fn mentions_ident(code: &str, ident: &str) -> bool {
    let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    code.match_indices(ident).any(|(i, _)| {
        boundary(code[..i].chars().next_back())
            && boundary(code[i + ident.len()..].chars().next())
    })
}

#[test]
fn test_rust_generation_modifiers_on_non_identity_auto_fields() {
    // #258: `&` / `^` on a NON-identity auto (`+`) field used to be silently
    // dropped — `indexed_fields` excluded every `auto_generate` field, so no index
    // was built and, for `&`, no uniqueness was enforced at all.  This affected
    // `+uuid` / `+timestamp`, which synthesize today, so it was a live bug rather
    // than one gated on #187.
    let src = r#"
Event {
  id: +uuid
  created_at: ^+timestamp
  ref_id: &+uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // `^` builds the index it asked for.
    assert!(
        mentions_ident(&code, "created_at_index"),
        "`^` on a non-identity auto field must build an index (#258)"
    );
    // `&` builds the index AND the uniqueness check that rejects a duplicate.
    assert!(
        mentions_ident(&code, "ref_id_index"),
        "`&` on a non-identity auto field must build an index (#258)"
    );
    // Flattened: prettyplease wraps the multi-field variant across lines and adds
    // a trailing comma, so match the field list rather than a formatted literal.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("ValidationError::Unique{model:\"Event\",field:\"ref_id\""),
        "`&` on a non-identity auto field must enforce uniqueness (#258)"
    );

    // The identity field keeps its exclusion: `id_to_row` already enforces it, so
    // a second index would be redundant.  This half of the old behavior is
    // correct and must not regress.
    assert!(
        !mentions_ident(&code, "id_index"),
        "the identity field must NOT get a redundant secondary index (#258)"
    );
}

#[test]
fn test_rust_generation_identity_modifiers_stay_redundant() {
    // #258 companion: the identity is excluded because it IS the identity, not
    // because it is auto-generated.  Both spellings of identity — a field named
    // `id`, and a differently-named `+` field standing in as the identity — build
    // no secondary index even when marked `&` / `^`.  (Schema validation warns and
    // suggests dropping the modifier; see the parser-side guard.)
    let src = r#"
Widget {
  id: &+uuid
  name: string
}

Gadget {
  code: ^+u64
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        !mentions_ident(&code, "id_index"),
        "`&` on an `id`-named identity stays redundant (#258)"
    );
    assert!(
        !mentions_ident(&code, "code_index"),
        "`^` on a `+`-field identity stays redundant (#258)"
    );
}

#[test]
fn test_rust_generation_single_and_composite_index_agree_on_auto_fields() {
    // #258: composite indexes never applied the auto-field exclusion, so before
    // the fix the SAME field was indexable via `@index(a, b)` but not via `^`.
    // That asymmetry is what proved the exclusion was an oversight rather than a
    // design choice, so guard the two paths against diverging again.
    let composite = r#"
Event {
  id: +uuid
  created_at: +timestamp
  name: string
  @index(created_at, name)
}
"#;
    let single = r#"
Event {
  id: +uuid
  created_at: ^+timestamp
  name: string
}
"#;
    let emit = |src: &str| {
        let mut parser = forgedb_parser::Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        RustGenerator::generate(&schema).unwrap().code
    };

    assert!(
        mentions_ident(&emit(composite), "created_at_name_index"),
        "a composite index over an auto field is built (pre-existing behavior)"
    );
    assert!(
        mentions_ident(&emit(single), "created_at_index"),
        "a single index over the same auto field must also be built (#258)"
    );
}

#[test]
fn test_rust_generation_txn_commit_journal() {
    // MVCC Tier 1 (#83, M1b): the atomic multi-model commit journal.  Commit fsyncs
    // every touched collection's columns + WAL BEFORE appending the journal record
    // (the ordering that makes the journal fsync the atomic commit point), and
    // reopen truncates every touched model back to its journalled committed length.
    // The journal reuses the published `forgedb-wal` `Raw` path — no new substrate.
    let src = r#"
User {
  id: +uuid
  email: &string
}

Post {
  id: +uuid
  author: *User
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Database holds the journal, opened at open_at via the published WAL Raw path.
    assert!(
        code.contains("_txn_journal: Option<forgedb_wal::WalManager>"),
        "Database owns the transaction commit journal"
    );
    assert!(
        code.contains("root.join(\"_txn_journal.log\")"),
        "journal is a root-level append-only log"
    );

    // Commit order: columns + WAL fsync FIRST, then the journal record + fsync.
    let commit_idx = code.find("pub fn commit(mut self) -> Result<(), TxError>").unwrap();
    let commit_body = &code[commit_idx..];
    let fsync_pos = commit_body.find(".commit();").expect("commit fsyncs columns");
    let journal_pos = commit_body
        .find("WalEntry::raw(\"_txn\"")
        .expect("commit writes the journal record");
    assert!(
        fsync_pos < journal_pos,
        "columns must be fsynced BEFORE the journal record (the atomic commit point)"
    );
    assert!(
        commit_body[journal_pos..].contains(".flush()"),
        "the journal record is fsynced (the single commit-point fsync)"
    );

    // Journal-driven recovery: open_at reads the last record and truncates each
    // touched collection back to its committed length via __recover_to_committed.
    assert!(
        code.contains("pub fn __recover_to_committed(&mut self, committed_len: usize)"),
        "generated per-model journal recovery"
    );
    assert!(
        code.contains("__db.user.__recover_to_committed(__len as usize)"),
        "open_at applies journalled committed lengths to each touched model"
    );

    // Identity: the journal payload is opaque bytes; recovery dispatches on the
    // opaque model tag, never a decoded field.
    assert!(
        code.contains("serde_json::to_vec(&__journal)"),
        "the journal record is opaque encoded bytes (length vector), not a decoded field"
    );
}

#[test]
fn test_rust_generation_txn_defers_maintenance() {
    // MVCC Tier 1 (#83, M1c/M1d, PM constraint 1): auto-checkpoint (#96) and
    // auto-compaction (#92) MUST NOT fire mid-transaction (a checkpoint would
    // truncate the per-model WAL underneath the rollback; a compaction would
    // renumber rows underneath the marks).  They are deferred behind an
    // `in_transaction` guard and run once after commit/rollback.
    let src = r#"
User {
  id: +uuid
  email: &string
  age: ^u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Storage carries the guard + deferred flags.
    assert!(
        code.contains("in_transaction: bool")
            && code.contains("checkpoint_deferred: bool")
            && code.contains("compact_deferred: bool"),
        "storage carries the txn guard + deferred flags"
    );

    // Auto-checkpoint defers while in a transaction.
    assert!(
        code.contains("if self.in_transaction {")
            && code.contains("self.checkpoint_deferred = true;"),
        "auto-checkpoint defers to checkpoint_deferred while in a transaction"
    );
    // Auto-compaction defers while in a transaction.
    assert!(
        code.contains("self.compact_deferred = true;"),
        "auto-compaction defers to compact_deferred while in a transaction"
    );
    // compact() itself early-returns mid-transaction (M1d).
    let compact_idx = code.find("pub fn compact(&mut self) {").unwrap();
    let compact_body = &code[compact_idx..compact_idx + 400];
    assert!(
        compact_body.contains("if self.in_transaction {")
            && compact_body.contains("self.compact_deferred = true;")
            && compact_body.contains("return;"),
        "compact() early-returns (deferred) when called mid-transaction"
    );

    // Deferred maintenance runs once after the critical section closes.
    assert!(
        code.contains("pub fn run_deferred_maintenance(&mut self)"),
        "a deferred checkpoint/compaction runs after commit/rollback"
    );
    assert!(
        code.contains("self.db.user.run_deferred_maintenance();"),
        "commit/rollback run the deferred maintenance per touched collection"
    );
}

#[test]
fn test_rust_generation_optimistic_commit() {
    // MVCC Tier 2 (#83): the generated Database gains `transaction_retrying` — an
    // optimistic, auto-retrying entry point with a sequencer-backed conflict map.
    // PM identity red line: the commit/conflict path must contain NO `match
    // model_name` (opaque-key discipline, same as the replication broker).
    let src = r#"
User {
  id: +uuid
  email: &string
}

Post {
  id: +uuid
  author: *User
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // transaction_retrying is generated on Database.
    assert!(
        flat.contains("pubfntransaction_retrying<T>(")
            && flat.contains("retries:u32,")
            && flat.contains("implFn(&mutTxHandle)->Result<T,TxError>"),
        "Database::transaction_retrying is generated"
    );

    // The sequencer is on Database (for LSN assignment + conflict detection).
    assert!(
        flat.contains("seq:std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>"),
        "Database carries the commit sequencer"
    );

    // Sequencer is seeded from the broker watermark on open_at.
    assert!(
        flat.contains("CommitSequencer::new(") && flat.contains("watermark()"),
        "sequencer seeded from broker watermark on open_at"
    );

    // The commit path calls try_commit with a WriteSet.
    assert!(
        flat.contains(".try_commit(&__ws)"),
        "transaction_retrying calls try_commit on the sequencer"
    );

    // The write-set is built via TxHandle::__write_set (opaque keys).
    assert!(
        flat.contains("fn__write_set(") || flat.contains("__write_set("),
        "TxHandle exposes __write_set to build the opaque write-set"
    );

    // Retry loop: after exhausting retries, returns TxError::Conflict.
    assert!(
        flat.contains("TxError::Conflict"),
        "exhausted retries return TxError::Conflict"
    );

    // transaction_optimistic is a convenience wrapper.
    assert!(
        flat.contains("pubfntransaction_optimistic<T>("),
        "transaction_optimistic convenience wrapper is generated"
    );

    // DEFAULT_TXN_RETRIES const.
    assert!(
        flat.contains("DEFAULT_TXN_RETRIES:u32"),
        "DEFAULT_TXN_RETRIES const is generated"
    );

    // PM identity red line: no match on model_name in the commit/conflict path.
    // (same discipline as test_rust_generation_replication_broker)
    let retrying_idx = flat.find("transaction_retrying").expect("transaction_retrying in generated code");
    let retrying_body = &flat[retrying_idx..retrying_idx + 2000.min(flat.len() - retrying_idx)];
    assert!(
        !retrying_body.contains("matchmodel_name"),
        "the commit/conflict path must never match on the model name"
    );

    // Tier 1 is intact: existing transaction() still generated.
    assert!(
        flat.contains("pubfntransaction<T>(")
            && flat.contains("implFnOnce(&mutTxHandle)->Result<T,TxError>"),
        "Tier 1 Database::transaction (FnOnce) is preserved"
    );

    // Tier 2 concurrent prepare: SharedDatabase, ConcurrentTxHandle, transaction_concurrent.
    assert!(
        flat.contains("pubstructSharedDatabase"),
        "SharedDatabase struct is generated"
    );
    assert!(
        flat.contains("pubstructConcurrentTxHandle"),
        "ConcurrentTxHandle struct is generated"
    );
    assert!(
        flat.contains("pubfntransaction_concurrent<T>("),
        "SharedDatabase::transaction_concurrent is generated"
    );
    // Private buffer staging (not shared columns).
    assert!(
        flat.contains("buffer:Vec<("),
        "ConcurrentTxHandle has a private buffer field"
    );
    // Write-set uses logical id bytes, not physical row index.
    // The old approach used `(*__row as u64).to_le_bytes()` — that string must not appear.
    assert!(
        flat.contains("__id_bytes"),
        "write-set uses logical id bytes"
    );
    assert!(
        !flat.contains("(*__rowasu64).to_le_bytes()"),
        "write-set must not use physical row index to_le_bytes"
    );
    // Database::shared() constructor.
    assert!(
        flat.contains("pubfnshared(self)->SharedDatabase"),
        "Database::shared() is generated"
    );
    // The concurrent apply dispatch is in the APPLY path only.
    assert!(
        flat.contains("__apply_and_commit_concurrent_buffer"),
        "Database::__apply_and_commit_concurrent_buffer is generated"
    );
}

#[test]
fn test_rust_generation_compaction_respects_live_snapshot() {
    // MVCC Tier 2 (#83, PM constraint 3): auto-compaction must not GC row versions
    // that a live transaction snapshot still needs.  In Tier 2 with &mut Database
    // semantics, the existing `in_transaction` flag transitively defers compaction
    // while any transaction is in flight (transaction_retrying takes &mut self and
    // calls TxHandle::begin which sets in_transaction on each touched collection).
    // This guard verifies that: (a) compact() still defers on in_transaction, and
    // (b) the sequencer's oldest_live_snapshot is checked at the Database level so
    // Tier 3 concurrent prepare is safe.
    let src = r#"
User {
  id: +uuid
  email: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // The in_transaction deferred guard is present in compact().
    assert!(
        code.contains("if self.in_transaction {") && code.contains("self.compact_deferred = true;"),
        "storage compact() defers while in_transaction is set"
    );

    // The sequencer is on Database; its oldest_live_snapshot transitively protects
    // live snapshots.  Verify the sequencer is accessible and the oldest_live_snapshot
    // method is referenced in the generated code (for the database-level compact guard).
    assert!(
        code.contains("oldest_live_snapshot"),
        "generated code references oldest_live_snapshot for the keep-set bound"
    );
}

#[test]
fn test_rust_generation_coordinated_client() {
    // Tier 3 MVCC coordinator client (#84): `Database::connect` must produce a
    // `CoordinatedDatabase` whose `transaction_coordinated` routes the serialized
    // commit section through the coordinator Unix socket while reusing the SAME
    // `__apply_and_commit_concurrent_buffer` data-plane path as Tier 2.
    //
    // PM identity constraints checked here (incl. the #84 lock-skip re-gate):
    // (a) The coordinator client surface is generated (no hand-rolled alternative).
    // (b) No second apply body — `transaction_coordinated` calls
    //     `__apply_and_commit_concurrent_buffer`, NOT a separate per-model dispatch.
    // (c) The coordinator receives only opaque bytes (no decoded field in the
    //     `Committed` payload: model tags are byte-cast, not field-decoded).
    // (d) `CoordinatedDatabase` is strictly additive — no existing method is modified.
    // (e) Real row indices forwarded — NOT placeholder 0 (T3-3/T3-8).
    // (f) Peer read-currency: `__peer_refresh` is generated data-plane code that
    //     reads shared column files; the coordinator has no column-write dep (T3-8).
    // (g) T3-8 structural: no `forgedb_storage` write import in coordinator payload
    //     build path (the generated Committed payload is opaque bytes only).
    // (G1) Lock-skip (T3-5): the coordinated open (`connect`) opens LOCK-FREE via
    //      `__open_with_lock(root, None)`; the standalone `open_at` DOES take the
    //      #89 `DirLock::acquire`. This pins the exact bug the #84 data-plane fix
    //      corrected (every client self-locking ⇒ concurrent multi-process
    //      impossible), analogous to the `no match model_name` structural guards.
    // (G3) T3-8 dependency: `forgedb-coordinator/Cargo.toml` has NO
    //      `forgedb-storage*` dependency (checked below), so the coordinator can
    //      never link the column-write machinery.
    let src = r#"
User {
  id: +uuid
  email: &string
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // (a) The CoordinatedDatabase struct is emitted.
    assert!(
        code.contains("pub struct CoordinatedDatabase"),
        "CoordinatedDatabase struct emitted"
    );

    // (a) Database::connect factory is emitted; the old lock-taking
    // connect_coordinator(self) form is GONE (it wrapped an already-locked
    // Database, which broke concurrent multi-process — #84).
    assert!(
        code.contains("pub fn connect("),
        "Database::connect emitted"
    );
    assert!(
        !code.contains("connect_coordinator"),
        "old lock-taking connect_coordinator removed (#84 lock-skip fix)"
    );

    // (a) The transaction method is emitted.
    assert!(
        code.contains("pub fn transaction_coordinated"),
        "CoordinatedDatabase::transaction_coordinated emitted"
    );

    // (b) No second apply body: the coordinator path delegates to the SAME
    // __apply_and_commit_concurrent_buffer that Tier 2 uses.
    assert!(
        code.contains("__apply_and_commit_concurrent_buffer"),
        "coordinator path reuses __apply_and_commit_concurrent_buffer (no drift)"
    );

    // (c) Opaque model tags — model name cast to bytes, NOT field-decoded.
    assert!(
        code.contains("as_bytes().to_vec()"),
        "model tags forwarded as opaque bytes, not decoded fields"
    );

    // (d) Existing Tier 2 surface is untouched: SharedDatabase + transaction_concurrent.
    assert!(
        code.contains("pub struct SharedDatabase"),
        "SharedDatabase still present (additive, no breakage)"
    );
    assert!(
        code.contains("pub fn transaction_concurrent"),
        "transaction_concurrent still present (additive, no breakage)"
    );

    // (d) The coordinator is referenced by its substrate path (not a generated struct).
    assert!(
        code.contains("forgedb_coordinator"),
        "generated code references the forgedb-coordinator substrate crate"
    );

    // (e) Real row indices: __apply_and_commit_concurrent_buffer returns Vec<(model_tag, row_index)>
    // and those are threaded into the Committed payload — NOT a placeholder 0 literal.
    assert!(
        !code.contains("__row_indices.push(0)"),
        "row_indices must NOT be placeholder 0 — real positions from __apply_and_commit (T3-3)"
    );
    // The pairs are used to build model_tags and row_indices from the Ok result.
    assert!(
        code.contains("__peer_refresh"),
        "peer read-currency: __peer_refresh method emitted (T3-8)"
    );

    // (f) Peer refresh re-derives EVERY column's row_count from disk (not just the
    // tombstone — that was the bug: a peer's row is unreadable until all columns'
    // bounds are refreshed), then folds ONLY the new rows into the maps (#161-B).
    assert!(
        code.contains("__sync_columns_from_disk"),
        "peer refresh syncs ALL columns from disk (not tombstone-only) — #84"
    );
    assert!(
        code.contains("sync_from_disk"),
        "peer refresh reads shared column live length via sync_from_disk (T3-8)"
    );
    // #161-B: the peer path captures the pre-sync watermark and folds only the
    // new rows `[from..row_count)` via the incremental `__reindex_delta` — NOT a
    // full clear-and-rescan (`__reindex_committed`) of every row × index.
    assert!(
        code.contains("let __from = self.user.row_count;")
            && code.contains("self.user.__reindex_delta(__from);"),
        "peer refresh folds only new rows via __reindex_delta (#161-B), not a full rebuild"
    );

    // (g) T3-8 structural: last_refreshed_lsn tracks the peer-refresh cursor.
    assert!(
        code.contains("last_refreshed_lsn"),
        "last_refreshed_lsn tracks peer refresh cursor (T3-8)"
    );

    // T3-5: Tier 2 call site is byte-identical — the `?;` expression is unchanged.
    // The apply function now returns Vec<...> but the Tier-2 caller still uses `?;`
    // (semicolon drops the Ok value).  Confirm SharedDatabase::transaction_concurrent
    // still calls __apply_and_commit_concurrent_buffer.
    assert!(
        code.contains("pub fn transaction_concurrent"),
        "Tier 2 transaction_concurrent unchanged (T3-5)"
    );

    // (G1) Lock-skip structural guard (#84 PM re-gate). The coordinated open path
    // is LOCK-FREE and the standalone path is NOT — the exact bug the data-plane
    // fix corrected. `connect` opens via `__open_with_lock(root, None)`; `open_at`
    // acquires the #89 DirLock.
    assert!(
        code.contains("__open_with_lock(root, None)"),
        "G1: coordinated open (connect) is LOCK-FREE — __open_with_lock(root, None)"
    );
    assert!(
        code.contains("DirLock::acquire"),
        "G1: standalone open_at still self-acquires the #89 DirLock"
    );
    // The lock-free open must be reachable ONLY through connect (which first
    // establishes a live coordinator) — surfaced as CoordinatorUnavailable if not.
    assert!(
        code.contains("CoordinatorUnavailable"),
        "G1: connect surfaces CoordinatorUnavailable (never a lock-free standalone writer)"
    );

    // (G3) T3-8 dependency guard — `forgedb-coordinator` must NOT depend on any
    // `forgedb-storage*` crate, so it can never link the column-write machinery
    // (`FixedColumn`/`VariableColumn`/`Tombstones` append). Read its manifest.
    let coord_manifest = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../coordinator/Cargo.toml"
    ))
    .expect("read forgedb-coordinator Cargo.toml");
    assert!(
        !coord_manifest.contains("forgedb-storage"),
        "G3 (T3-8): forgedb-coordinator must have NO forgedb-storage* dependency"
    );
}

/// #274 — both coordinator error arms must drop the connection before returning.
///
/// A failed `request_turn` or `committed` leaves the coordinator's reply **in
/// flight**; it will still be written onto that socket. Without a reconnect the
/// *next* transaction reads it as its own answer, and a stale `Grant` read that way
/// makes the generated data plane write columns for a turn the coordinator has
/// already reclaimed — reported as `Ok`, because the resulting `Error` on
/// `Committed` lands in the `eprintln!` arm.
///
/// The substrate poisons the connection so that misread is impossible, but poison
/// alone would strand the process: `Arc<CoordinatorClient>` is built once in
/// `CoordinatedDatabase::connect` and never rebuilt, so every later write would fail
/// until the app reconstructed the whole database. The `reconnect()` calls asserted
/// here are what make the poisoning recoverable, and they live in **generated** code
/// on purpose — beside the `Busy` retry budget, which is where this project keeps
/// recovery policy rather than in the substrate.
///
/// Guarded structurally rather than by snapshot because a snapshot accepts a
/// deletion here as readily as an addition.
#[test]
fn test_rust_generation_coordinator_errors_reconnect() {
    let src = r#"
Ticket {
  id: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    let reconnects = code.matches("__coord.reconnect()").count();
    assert_eq!(
        reconnects, 2,
        "expected exactly two `__coord.reconnect()` calls — one in the request_turn \
         error arm, one in the Committed ack error arm; found {reconnects}"
    );

    // The request_turn arm: reconnect must precede the return, or the connection is
    // handed to the next transaction still desynchronized.
    let req_arm = code
        .find("__coord.reconnect()")
        .expect("first reconnect present");
    let io_return = code[req_arm..]
        .find("TxError::Io")
        .expect("the request_turn arm still returns TxError::Io");
    assert!(
        io_return < 400,
        "the first reconnect() must sit in the request_turn error arm, immediately \
         before its `return Err(TxError::Io(..))`"
    );

    // The Committed arm keeps returning Ok — the commit really is durable (columns +
    // WAL are fsynced before `Committed` is sent), so a missing ack must not turn a
    // successful commit into a reported failure.
    assert!(
        code.contains("coordinator: Committed ack error"),
        "the Committed ack arm keeps its diagnostic and does not become fatal"
    );
}


// ---------------------------------------------------------------------------
// #74 Phase 3 — the offline transformer bin (uniform typed replay).
// The identity trio (DV-1): no schema at runtime (C1), straight-line replay with
// no interpreter (C2/C8/DV-11), provider-free deps (C4/DV-7).
// ---------------------------------------------------------------------------

/// Parse a `.forge` source into a schema (test helper).
fn parse_forge(src: &str) -> Schema {
    let mut p = forgedb_parser::Parser::new(src).unwrap();
    p.parse().unwrap()
}

/// A 3-version lineage mixing an additive `AutoBody` hop (v1→v2: add `bio:
/// string?`) and a semantic `AuthoredBody` hop (v2→v3: `age` u32→string). This is
/// exactly the E2E shape from the impl plan; the guards below inspect the emitted
/// transformer crate for it.
fn sample_transform_crate() -> (String, forgedb_codegen::TransformCrate) {
    let v1 = parse_forge("User {\n  id: +uuid\n  age: u32\n}\n");
    let v2 = parse_forge("User {\n  id: +uuid\n  age: u32\n  bio: string?\n}\n");
    let v3 = parse_forge("User {\n  id: +uuid\n  age: string\n  bio: string?\n}\n");
    // Keep the parsed schemas alive by leaking them for the borrow — this is a
    // one-shot test helper, so the leak is harmless.
    let v1: &'static Schema = Box::leak(Box::new(v1));
    let v2: &'static Schema = Box::leak(Box::new(v2));
    let v3: &'static Schema = Box::leak(Box::new(v3));

    let plan = TransformPlan {
        versions: vec![
            VersionSchema { version: 1, schema: v1 },
            VersionSchema { version: 2, schema: v2 },
            VersionSchema { version: 3, schema: v3 },
        ],
        hops: vec![
            HopPlan {
                from_version: 1,
                to_version: 2,
                migration_id: "m1".to_string(),
                model_ops: vec![ModelOp {
                    model: "User".to_string(),
                    source_model: "User".to_string(),
                    field_renames: vec![],
                    field_removes: vec![],
                    field_adds: vec![("bio".to_string(), "null".to_string())],
                }],
                authored_src: None,
            },
            HopPlan {
                from_version: 2,
                to_version: 3,
                migration_id: "m2".to_string(),
                model_ops: vec![],
                authored_src: Some(
                    "pub fn authored_transform(model: &str, mut row: serde_json::Value) \
                     -> serde_json::Value {\n    if model == \"User\" {\n        if let Some(v) = \
                     row.get(\"age\").and_then(|x| x.as_u64()) {\n            row[\"age\"] = \
                     serde_json::Value::String(v.to_string());\n        }\n    }\n    row\n}\n"
                        .to_string(),
                ),
            },
        ],
    };
    let crate_out = TransformGenerator::generate(&plan, "forgedb-transform").unwrap();
    let main = crate_out
        .sources
        .iter()
        .find(|(p, _)| p == "src/main.rs")
        .map(|(_, c)| c.clone())
        .expect("main.rs emitted");
    (main, crate_out)
}

#[test]
fn test_transform_bin_has_no_schema_runtime() {
    // C1 / DV-1: the transformer embeds fixed typed version modules; its replay
    // path constructs no decoder from runtime input — no `.forge` parse, no
    // parser/migrations symbol.
    let (main, _crate_out) = sample_transform_crate();
    assert!(
        !main.contains("forgedb_parser") && !main.contains("forgedb_migrations"),
        "transformer main must not link the parser / migration engine at runtime"
    );
    assert!(
        !main.contains("SimpleSchema")
            && !main.contains("Parser::new")
            && !main.contains("schema.forge"),
        "transformer must not read or interpret a schema at runtime"
    );
    // It DOES call into the embedded per-version typed databases.
    assert!(
        main.contains("v1::Database") && main.contains("v3::Database"),
        "replay reads/writes via the embedded per-version typed structs"
    );
}

#[test]
fn test_transform_bin_replay_is_straightline() {
    // C2 / C8 / DV-11: `run` is a fixed named-hop call-chain, not a loop over a
    // persisted plan descriptor and not a runtime mechanism-selection branch.
    let (main, _crate_out) = sample_transform_crate();
    assert!(main.contains("fn run("), "a fixed run() entrypoint");
    assert!(
        main.contains("fn transform_v1_to_v2(") && main.contains("fn transform_v2_to_v3("),
        "one named hop fn per adjacent version pair"
    );
    assert!(
        main.contains("transform_v1_to_v2(") && main.contains("transform_v2_to_v3("),
        "run() calls the named hops directly (straight-line chain)"
    );
    // No descriptor interpreter / no runtime hop-class dispatch.
    for forbidden in [
        "Vec<Step",
        "HopDescriptor",
        "for step in",
        "match change",
        "match hop",
        "match from",
    ] {
        assert!(
            !main.contains(forbidden),
            "replay must not interpret a persisted plan / dispatch a mechanism at runtime (found {forbidden:?})"
        );
    }
}

#[test]
fn test_transform_bin_deps_are_provider_free() {
    // C4 / DV-7: the emitted Cargo.toml links the schema-agnostic substrate the
    // app links, and NOTHING that interprets a schema — no parser, no migration
    // engine.
    let (_main, crate_out) = sample_transform_crate();
    let toml = &crate_out.cargo_toml;
    assert!(
        !toml.contains("forgedb-migrations =") && !toml.contains("forgedb-parser ="),
        "transformer must be provider-free (no parser / migration crate dependency)"
    );
    assert!(
        toml.contains("forgedb-storage") && toml.contains("forgedb-types"),
        "transformer links the same substrate the generated app links"
    );
}

#[test]
fn test_transform_bin_embeds_frozen_authored_body() {
    // C13: an authored hop embeds its frozen `transform.rs` verbatim as a module
    // and the hop calls it; an auto hop does not.
    let (main, crate_out) = sample_transform_crate();
    assert!(
        crate_out.sources.iter().any(|(p, _)| p == "src/authored_m2.rs"),
        "the authored hop's frozen body is embedded as its own module"
    );
    assert!(
        main.contains("mod authored_m2;") && main.contains("authored_m2::authored_transform"),
        "the v2→v3 hop declares + calls the frozen authored transform"
    );
    assert!(
        !main.contains("authored_m1"),
        "the auto v1→v2 hop embeds no authored body"
    );
    // The additive hop bakes the frozen field-add op (bio defaults to null).
    assert!(
        main.contains("\"bio\"") && main.contains("v2::User"),
        "the additive hop inserts the new field's default and decodes v2::User"
    );
}

#[test]
fn test_transform_generation_snapshot() {
    let (main, _crate_out) = sample_transform_crate();
    insta::assert_snapshot!(main);
}

/// The multi-model + FK + M2M + integer-PK schema shared by the Go binding
/// guard tests (mirrors the PyO3/NAPI fixture so coverage is comparable).
fn go_binding_schema() -> Schema {
    let src = r#"
enum Status { Draft, Published }

User {
  id: +uuid
  email: &string
  age: i32?
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  status: Status
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  label: string
  posts: [Post]
}

Reading {
  id: +u64
  value: i64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    parser.parse().unwrap()
}

#[test]
fn test_go_generation_binding() {
    // Golang binding (RFC #203) — the per-schema cgo wrapper over the SAME
    // generated FFI C-ABI. Schema-tailored Go structs + typed methods that
    // reference the generated per-model C symbols by name; rows/ids cross cgo as
    // opaque JSON. No generic query builder, no `switch model` (the red line).
    let schema = go_binding_schema();
    let code = GoGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // Package + cgo boundary + the schema-invariant lifecycle spine.
    assert!(code.contains("package forgedb"), "emits a Go package");
    assert!(code.contains("import \"C\""), "binds the C ABI over cgo");
    // De-experimentalized (#204): the generated Go no longer carries an
    // experimental/unstable disclaimer (it is at NAPI-RS parity).
    assert!(
        !code.to_uppercase().contains("EXPERIMENTAL"),
        "no experimental marker after de-experimentalization (#204)"
    );
    assert!(code.contains("func Open(root string) (*DB, error)"), "the Open lifecycle entry");
    assert!(flat.contains("func(db*DB)Commit()error"), "Commit wraps forgedb_commit");
    assert!(flat.contains("C.forgedb_open("), "Open calls forgedb_open");
    assert!(flat.contains("C.forgedb_free_buffer("), "buffers freed via forgedb_free_buffer");

    // One row struct per model; typed fields; nullable FK/scalar handling.
    for m in ["User", "Post", "Tag", "Reading"] {
        assert!(code.contains(&format!("type {m} struct {{")), "row struct for {m}");
    }
    // `+uuid` id carries omitempty (serde default lets a create body omit it).
    assert!(code.contains("Id string `json:\"id,omitempty\"`"), "uuid id is an omitempty string");
    // `i32?` nullable scalar → pointer.
    assert!(code.contains("Age *int32 `json:\"age\"`"), "nullable i32 maps to *int32");
    // `*User` FK is stored as the uuid reference id → string.
    assert!(code.contains("Author string `json:\"author\"`"), "required FK maps to a string id");
    // integer-PK model's id type flows through.
    assert!(code.contains("Id uint64 `json:\"id,omitempty\"`") || code.contains("Id uint64 `json:\"id\"`"), "u64 PK id is uint64");

    // Per-model CRUD + snapshot reads, each keyed by the model's snake symbol.
    for (name, snake) in [("User", "user"), ("Post", "post"), ("Reading", "reading")] {
        for op in ["Insert", "Get", "Count", "All", "Update", "Delete"] {
            assert!(code.contains(&format!("func (db *DB) {op}{name}(")), "method {op}{name}");
        }
        assert!(code.contains(&format!("C.forgedb_{snake}_insert(")), "insert calls the C symbol");
        assert!(code.contains(&format!("func (db *DB) Get{name}At(")), "snapshot _at read for {name}");
    }

    // Relation traversal: forward FK, reverse 1:M, and M2M getters/link/unlink.
    assert!(code.contains("func (db *DB) PostAuthor(") && flat.contains("C.forgedb_post_author("),
        "forward FK Post.author");
    assert!(code.contains("func (db *DB) UserPosts(") && flat.contains("C.forgedb_user_posts("),
        "reverse 1:M User.posts");
    assert!(flat.contains("C.forgedb_link_post_tag(") && flat.contains("C.forgedb_unlink_post_tag("),
        "M2M link/unlink for Post<->Tag");
    assert!(code.contains("func (db *DB) Link"), "an M2M Link method is generated");

    // Typed rows: enums map to a generated `type <Name> string` + consts; the
    // enum-typed field uses it (not a bare string / json.RawMessage).
    assert!(code.contains("type Status string"), "enum generates a named Go type");
    assert!(code.contains("StatusDraft Status = \"Draft\""), "enum variant const");
    assert!(code.contains("Status Status `json:\"status\"`"), "enum-typed field uses the enum type");

    // Async CRUD over the completion bridge: generic Result[T] + per-model methods
    // returning channels, plus the exported-callback registration shim.
    assert!(code.contains("type Result[T any] struct"), "generic async Result type");
    assert!(code.contains("func runAsync["), "the generic async driver");
    assert!(code.contains("C.forgedbGoRegister()"), "registers the completion callback");
    for (name, snake) in [("User", "user"), ("Post", "post"), ("Reading", "reading")] {
        for (op, sym) in [
            ("Insert", "insert"), ("Get", "get"), ("Count", "count"),
            ("All", "all"), ("Update", "update"), ("Delete", "delete"),
        ] {
            assert!(code.contains(&format!("func (db *DB) {op}{name}Async(")), "async {op}{name}");
            assert!(code.contains(&format!("C.forgedb_{snake}_{sym}_async(")), "async C symbol {snake}_{sym}");
        }
    }

    // M2M snapshot-scoped `_at` traversal getter (takes a *Snapshot).
    assert!(code.contains("snap *Snapshot, id string) ([]"), "an M2M _at getter over a snapshot");

    // Arrow columnar export (separate file; only when exportable columns exist).
    assert!(GoGenerator::needs_arrow(&schema), "schema has arrow-exportable columns");
    let arrow = GoGenerator::generate_arrow(&schema).expect("arrow file emitted").code;
    assert!(arrow.contains("arrow-go/v18/arrow/cdata"), "imports arrow-go cdata");
    assert!(arrow.contains("cdata.ImportCArray("), "imports the FFI C-Data-Interface export");
    assert!(
        arrow.contains("func (db *DB) ExportReadingValueArrow() (arrow.Array, error)"),
        "a per-column Export<Model><Field>Arrow method (i64 value column)"
    );
    assert!(arrow.contains("C.forgedb_reading_value_export_arrow("), "calls the FFI export symbol");
    // The arrow-go require lands in go.mod only when needed.
    assert!(GoGenerator::go_mod_scaffold("forgedb", true).contains("arrow-go/v18"), "go.mod pins arrow-go when needed");
    assert!(!GoGenerator::go_mod_scaffold("forgedb", false).contains("arrow-go"), "no arrow dep when unneeded");

    // Identity red line: no generic runtime query surface, ever.
    for forbidden in ["forgedb_query", "switch model", "predicate", "QueryBuilder", "reflect."] {
        assert!(!code.contains(forbidden), "must not emit generic query surface: {forbidden}");
    }
}

#[test]
fn test_go_calls_match_ffi_symbols() {
    // Anti-drift guard: every `C.forgedb_*` symbol the generated Go binding calls
    // MUST be a symbol the FFI generator actually emits for the same schema. The
    // Go generator re-derives relation names independently (mirroring the FFI
    // derivation); this proves the two never disagree, so the Go binding can
    // never reference a nonexistent C symbol.
    let schema = go_binding_schema();
    // Concatenate every generated Go file so the guard also covers the arrow +
    // async-bridge C calls, not just the main package file.
    let mut go_code = GoGenerator::generate(&schema).unwrap().code;
    go_code.push_str(&GoGenerator::generate_async_bridge().code);
    if let Some(arrow) = GoGenerator::generate_arrow(&schema) {
        go_code.push_str(&arrow.code);
    }
    let ffi_flat: String = FfiGenerator::generate(&schema)
        .unwrap()
        .code
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    // Extract every `C.forgedb_<name>` symbol referenced by the Go binding.
    let mut go_symbols: Vec<String> = Vec::new();
    let bytes = go_code.as_bytes();
    let needle = b"C.forgedb_";
    let mut i = 0;
    while let Some(pos) = go_code[i..].find("C.forgedb_") {
        let start = i + pos + 2; // skip "C."
        let mut end = start + (needle.len() - 2); // past "forgedb_"
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        go_symbols.push(go_code[start..end].to_string());
        i = end;
    }
    go_symbols.sort();
    go_symbols.dedup();
    assert!(
        go_symbols.len() > 10,
        "expected the Go binding to reference many C symbols, found {}",
        go_symbols.len()
    );

    // Each must appear as an `extern \"C\" fn <sym>(` in the FFI output.
    for sym in &go_symbols {
        assert!(
            ffi_flat.contains(&format!("fn{sym}(")),
            "Go calls C.{sym} but the FFI generator emits no such symbol (drift!)"
        );
    }
}

// ---------------------------------------------------------------------------
// REST client SDKs (#118 Python / #205 Go / #206 Rust): full-parity siblings of
// the TypeScript SDK. One shared schema exercises every codepath the parity
// surface touches — enum, @projection, required/optional FK, one-to-many virtual
// relation, decimal, json (+ nullable json), nullable scalar, +auto fields, and a
// multi-word model name (kebab-case). Named asserts guard the load-bearing
// surface (CRUD + projection methods, error/list types, field-type mapping, and
// the #188-correct create-input that omits ONLY server-synthesized +uuid/
// +timestamp autos); the snapshot locks the exact output.
// ---------------------------------------------------------------------------

/// The shared SDK-parity fixture, parsed from source for readability.
fn sdk_parity_schema() -> Schema {
    let src = r#"
        enum Role { Admin, Member, Guest }

        Account {
            id: +uuid
            email: &string @email
            role: Role
            balance: decimal
            metadata: json?
            bio: string?
            login_count: u64
            created_at: +timestamp
            projects: [Project]
            @projection(summary: email, role)
        }

        Project {
            id: +uuid
            name: string
            owner: *Account
            reviewer: ?Account
            tags: json
            priority: i32?
            @projection(card: name)
        }

        AuditEvent {
            id: +uuid
            kind: string
            at: +timestamp
        }
    "#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    parser.parse().unwrap()
}

#[test]
fn test_rust_sdk_generation_snapshot() {
    let schema = sdk_parity_schema();
    let code = RustSdkGenerator::generate(&schema).unwrap().code;

    // Enum + model + create + projection types.
    assert!(code.contains("pub enum Role {"), "enum type missing");
    assert!(code.contains("pub struct Account {"), "model struct missing");
    assert!(code.contains("pub struct AccountSummary {"), "projection struct missing");

    // Full CRUD surface (snake_case methods).
    for m in ["get_account", "list_account", "create_account", "update_account", "delete_account"] {
        assert!(code.contains(&format!("pub async fn {m}(")), "missing method {m}");
    }
    // Projection read methods.
    assert!(code.contains("pub async fn get_account_summary("), "projection get missing");
    assert!(code.contains("pub async fn list_account_summary("), "projection list missing");

    // Shared surface.
    assert!(code.contains("pub struct ForgeDbError"), "typed error missing");
    assert!(code.contains("ListResult<Account>"), "paginated list result missing");

    // Field-type mapping: FK -> uuid String (required) / Option<String> (optional);
    // decimal -> String; json + one-to-many virtual relation -> opaque Value.
    assert!(code.contains("pub owner: String"), "required FK should be String");
    assert!(code.contains("pub reviewer: Option<String>"), "optional FK should be Option<String>");
    assert!(code.contains("pub balance: String"), "decimal should be String");
    assert!(code.contains("pub tags: serde_json::Value"), "json should be opaque Value");
    assert!(code.contains("pub projects: serde_json::Value"), "virtual relation should be opaque Value");

    // Multi-word model -> kebab-case route.
    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

    // #188-correct create input: AuditEvent's create omits BOTH +auto fields
    // (id: +uuid, at: +timestamp), leaving only `kind`.
    assert!(
        code.contains("pub struct AuditEventCreate {\n    pub kind: String,\n}"),
        "create input must omit +uuid/+timestamp autos (got:\n{})",
        &code[code.find("pub struct AuditEventCreate").unwrap_or(0)..]
            .chars().take(120).collect::<String>()
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_go_sdk_generation_snapshot() {
    let schema = sdk_parity_schema();
    let code = GoSdkGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("type Role string"), "string-typed enum missing");
    assert!(code.contains("RoleAdmin Role = \"Admin\""), "enum const missing");
    assert!(code.contains("type Account struct {"), "model struct missing");
    assert!(code.contains("type AccountSummary struct {"), "projection struct missing");

    // Full CRUD surface (exported PascalCase methods).
    for m in ["GetAccount", "ListAccount", "CreateAccount", "UpdateAccount", "DeleteAccount"] {
        assert!(code.contains(&format!("func (c *Client) {m}(")), "missing method {m}");
    }
    assert!(code.contains("func (c *Client) GetAccountSummary("), "projection get missing");
    assert!(code.contains("func (c *Client) ListAccountSummary("), "projection list missing");

    assert!(code.contains("type ForgeDbError struct"), "typed error missing");
    assert!(code.contains("ListResult[Account]"), "generic list result missing");

    // FK -> string / *string; decimal -> string; json + virtual -> json.RawMessage.
    assert!(code.contains("Owner string `json:\"owner\"`"), "required FK should be string");
    assert!(code.contains("Reviewer *string `json:\"reviewer\"`"), "optional FK should be *string");
    assert!(code.contains("Balance string `json:\"balance\"`"), "decimal should be string");
    assert!(code.contains("Tags json.RawMessage `json:\"tags\"`"), "json should be RawMessage");
    assert!(code.contains("Projects json.RawMessage `json:\"projects\"`"), "virtual relation opaque");

    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

    // #188-correct create input.
    assert!(
        code.contains("type AuditEventCreate struct {\n\tKind string `json:\"kind\"`\n}"),
        "create input must omit +uuid/+timestamp autos"
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_python_sdk_generation_snapshot() {
    let schema = sdk_parity_schema();
    let code = PythonSdkGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("class Role(str, Enum):"), "str Enum missing");
    assert!(code.contains("class Account:"), "model dataclass missing");
    assert!(code.contains("class AccountSummary:"), "projection dataclass missing");

    // Full CRUD surface (snake_case methods).
    for m in ["get_account", "list_account", "create_account", "update_account", "delete_account"] {
        assert!(code.contains(&format!("def {m}(")), "missing method {m}");
    }
    assert!(code.contains("def get_account_summary("), "projection get missing");
    assert!(code.contains("def list_account_summary("), "projection list missing");

    assert!(code.contains("class ForgeDbError(Exception):"), "typed error missing");
    assert!(code.contains("class ListResult(Generic[T]):"), "list result missing");

    // FK -> str / Optional[str]; decimal -> str; json + virtual -> Any.
    assert!(code.contains("owner: str"), "required FK should be str");
    assert!(code.contains("reviewer: Optional[str] = None"), "optional FK should be Optional[str]");
    assert!(code.contains("balance: str"), "decimal should be str");
    assert!(code.contains("tags: Any = None"), "json should be Any (defaulted)");
    assert!(code.contains("projects: Any = None"), "virtual relation should be Any (defaulted)");

    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

    // #188-correct create input: AuditEventCreate carries only `kind`.
    let create_idx = code.find("class AuditEventCreate:").expect("create dataclass present");
    let create_block: String = code[create_idx..].chars().take(200).collect();
    assert!(create_block.contains("kind: str"), "create keeps required kind");
    assert!(!create_block.contains("id:") && !create_block.contains("at:"),
        "create must omit +uuid/+timestamp autos, got:\n{create_block}");

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_borrowed_scan_view() {
    // #224: the narrow scan decodes each live row into a BORROWED view whose strings
    // point straight at the buffered span.  Before this, every live row's strings
    // were allocated and copied out of that span, then most of them were thrown away
    // by the list handler's `retain`.
    //
    // #228: and the borrowed view is now the ONLY scan view.  The owned twin existed
    // solely because the scan handed its rows back to the caller, so the borrows had
    // to be broken before the buffers dropped — and on an unfiltered list every row
    // was a survivor, so every row still paid the copy.  Making the scan a scope
    // removes the reason for the copy rather than narrowing who pays it.
    let src = r#"
User {
  id: +uuid
  email: &string
  bio: string?
  age: ^u32
  score: f64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // The borrowed twin of the scan record: strings borrow from the buffered span,
    // every other filterable field keeps the owned record's exact type (that is
    // what lets ONE emitted filter compile against both views).
    assert!(
        flat.contains(
            "pub struct UserScanRef<'a> { pub id: Uuid, pub email: &'a str, \
             pub bio: Option<&'a str>, pub age: u32, pub score: f64, }"
        ),
        "UserScanRef borrows strings and mirrors every other scan field type.\nGot: {flat}"
    );
    // Internal by decision (#224): never a wire type, so no Serialize/ToSchema on it.
    let ref_at = flat.find("pub struct UserScanRef").expect("has UserScanRef");
    let ref_decl = &flat[ref_at.saturating_sub(120)..ref_at];
    assert!(
        !ref_decl.contains("Serialize") && !ref_decl.contains("ToSchema"),
        "UserScanRef must stay internal — no wire derives.\nGot: {ref_decl}"
    );

    // The buffered scan reads strings with `read_str` (borrowed) — the whole point.
    assert!(
        flat.contains(".email_col .read_str(__slot)"),
        "buffered scan must borrow the string slot via read_str.\nGot: {flat}"
    );
    // The nullable arm slices past the presence tag WITHOUT allocating.
    assert!(
        flat.contains("Some(&raw[1..])"),
        "nullable string borrows past the presence tag instead of copying"
    );

    // The full-record positional decode (`read_at`, which the page materialization
    // goes through) is untouched — it still allocates, because its callers need
    // owned records.  Only `limit` rows reach it.
    assert!(
        flat.contains(".email_col .read_string(row_index)"),
        "the positional read path keeps the owned decode"
    );

    // #228: one scan entry point, and it is a SCOPE.  `keep` runs during decode so a
    // rejected row is never pushed (that is #224's win, preserved); `f` runs while
    // the buffers are alive, and only what it returns escapes.
    assert!(
        flat.contains(
            "pub fn __with_scan<R>( &self, sel: Option<Vec<usize>>, \
             keep: impl Fn(&UserScanRef<'_>) -> bool, \
             f: impl FnOnce(&mut Vec<UserScanRef<'_>>) -> R, ) -> R"
        ),
        "__with_scan is the scan scope: a selection, a predicate, and a callback.\nGot: {flat}"
    );
    // Survivors are collected as BORROWED views — no owned row, no `to_owned_row`.
    assert!(
        flat.contains("if keep(&__row_ref) { __refs.push(__row_ref); }"),
        "survivors stay borrowed inside the scope.\nGot: {flat}"
    );
    // The scope's return is the callback's return, so nothing borrowed can leak: the
    // view's lifetime is higher-ranked, which is what makes `R` unable to name it.
    assert!(
        flat.contains("f(&mut __refs) }"),
        "the scope returns only what the callback returns.\nGot: {flat}"
    );
}

#[test]
fn test_api_generation_borrowed_scan_filter() {
    // #224: the list filter and the live-query re-run evaluate on the borrowed
    // scan view, so a row they reject never allocates its strings.
    let src = r#"
User {
  id: +uuid
  email: &string
  bio: string?
  age: ^u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = ApiGenerator::generate(&schema).unwrap().code;
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // #228: exactly ONE scan filter, over the borrowed view.  #224 had emitted a
    // second, owned-operand copy for the index-pushdown arm, which was the only
    // caller that held no buffered span; unifying that arm on the scan scope left it
    // with none.
    assert!(code.contains("fn __user_scan_matches("), "scan filter emitted");
    assert!(!code.contains("__user_scan_matches_ref"), "the owned-operand twin is gone");
    assert!(
        flat.contains(
            "fn __user_scan_matches( record: &super::UserScanRef<'_>, \
             params: &HashMap<String, String>, ) -> bool"
        ),
        "the one scan filter takes the borrowed view.\nGot: {flat}"
    );

    // The REST list source and BOTH live-query scans filter during the scan
    // instead of decoding everything and `retain`ing afterwards.
    assert!(
        flat.contains("__with_scan( None, |r| __user_scan_matches(r, &params),"),
        "the live-query scans filter on the borrowed view inside the scope.\nGot: {flat}"
    );
    // All three call sites: the REST list source, the live-query initial scan, and
    // the live-query re-run.  Since #228 the pushdown fallback is no longer a fourth
    // *scan* — it is a `None` selection into the same one — so this is exact.
    assert!(
        flat.matches("|r| __user_scan_matches(r, &params)").count() == 3,
        "REST list + live-query init + live-query re-run, one scan each.\nGot: {flat}"
    );
    assert!(
        !flat.contains(".retain(|r| __user_scan_matches(r, &params))"),
        "no post-scan retain over decoded rows remains"
    );

    // The comparison that made the borrowed view non-trivial: `Option`'s PartialEq is
    // homogeneous, so there is no `Option<&str> == Option<String>` and a nullable
    // string has to compare against a BORROWED param.  Non-nullable strings need no
    // adjustment — std has `&str: PartialEq<String>`.  That asymmetry is not obvious
    // from either type, so it stays pinned even though the owned twin it used to be
    // contrasted against is gone.
    let matcher = &code[code.find("fn __user_scan_matches(").unwrap()..];
    assert!(
        matcher.contains("record.bio == Some(__w.as_str())"),
        "nullable string compares Option<&str> against a borrowed param.\nGot: {matcher}"
    );
    assert!(
        matcher.contains("record.email == __w"),
        "non-nullable string compares &str against the owned param directly"
    );
}

/// #230: index keys are emitted **monomorphically per field type**, not by matching
/// on a `serde_json::Value` at runtime.
///
/// This is link 1 of the parity guard chain — it pins *what the generator emits*.
/// Link 2 (`tests/index_key_parity_test.rs`) compiles a generated crate and asserts
/// those forms are byte-identical to the `Value` match they replaced. Changing
/// `RustGenerator::index_key_body` means moving both.
#[test]
fn test_rust_generation_monomorphic_index_keys() {
    let src = r#"
enum Status { Draft, Published }

Kitchen {
  id: +uuid
  s_name: &string
  s_code: ^bytes(8)
  n_u32: ^u32
  n_f64: ^f64
  b_flag: ^bool
  d_price: ^decimal
  u_ref: ^uuid
  t_at: ^timestamp
  e_status: ^Status
  o_name: ^string?
  owner: *Owner
  editor: ?Owner
}

Owner {
  id: +uuid
  email: &string
  kitchens: [Kitchen]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // The whole point: no index key is built by dispatching on a `Value` variant.
    // `is_filterable_scalar` + the two FK relations are fully covered by
    // `index_key_body`, so the generic fallback must never be reached.
    assert!(
        !flat.contains("matchserde_json::to_value"),
        "no index key may route through a serde_json::Value match (#230).\nGot: {code}"
    );
    // The dead `Err` arm goes with it — `to_value` cannot fail for any indexable
    // type (non-finite floats become `Value::Null`, they do not error), so the
    // `\\u{3}` tag was unreachable and is no longer emitted anywhere.
    assert!(
        !flat.contains(r"String::from('\u{3}')"),
        "the unreachable serialization-error tag must no longer be emitted (#230)"
    );

    // --- JSON string class (tag \u{1}) ------------------------------------
    assert!(
        flat.contains(r"__k.push('\u{1}');__k.push_str(__v);"),
        "string keys copy the value straight in, with no intermediate allocation"
    );
    assert!(
        flat.contains("__v.hyphenated().encode_lower(&mut__buf)"),
        "uuid / FK keys render hyphenated-lowercase into a stack buffer"
    );
    assert!(
        flat.contains("__v.__as_str()"),
        "enum keys use the generated variant-name accessor, matching the serde derive"
    );
    assert!(
        flat.contains(r#"letmut__k=String::from('\u{1}');let_=write!(__k,"{}",__v);"#),
        "decimal keys write the Display form (== its serde string form) in place"
    );

    // --- JSON non-string scalar class (tag \u{2}) -------------------------
    assert!(
        flat.contains(r#"letmut__k=String::from('\u{2}');let_=write!(__k,"{}",__v);"#),
        "integer keys write digits straight in"
    );
    assert!(
        flat.contains(r#"write!(__k,"{}",__v.as_micros())"#),
        "#254: the INDEX key stays the bare stored number and stays in the numeric \
         class — the RFC 3339 change is the wire form only, and keying on the \
         string would silently reorder every timestamp index lexicographically"
    );
    assert!(
        flat.contains(r#"__k.push_str(if*__v{"true"}else{"false"});"#),
        "bool keys are a literal, not a formatted value"
    );
    // Floats key off their total-order u64 encoding (#242), NOT any float
    // rendering. JSON has no form for a non-finite, which is why the old
    // `serde_json::Number::from_f64` path returned None for NaN/±Inf and dropped
    // them into the null bucket.
    assert!(
        flat.contains(r#"write!(__k,"{}",__forgedb_f64_key(*__v))"#),
        "f64 keys must be the total-order encoding (#242)"
    );
    assert!(
        !flat.contains("serde_json::Number::from_f64"),
        "the from_f64 path is the #242 defect — it must be gone entirely"
    );
    // The helper the arm calls, with both canonicalizations — without them the
    // encoding is a faithful bijection over bit patterns, which is wrong for the
    // two cases where distinct patterns are equal values: -0.0 == 0.0, and every
    // NaN payload.
    assert!(
        flat.contains("fn__forgedb_f64_key(__v:f64)->u64"),
        "the total-order key helper must be emitted"
    );
    assert!(
        flat.contains("let__v=if__v==0.0{0.0}elseif__v.is_nan(){f64::NAN}else{__v};"),
        "-0.0 and NaN payloads must be canonicalized before encoding"
    );
    assert!(
        flat.contains("let__mask=((__bitsasi64>>63)asu64)|0x8000_0000_0000_0000;"),
        "the sign-extended mask is what makes the encoding order-preserving"
    );
    // bytes(N) is [u8; N] — serde renders it as a JSON array, so the key is
    // `\u{2}[104,101,...]`, the non-string arm.
    assert!(
        flat.contains(r#"__k.push('[');for(__i,__b)in__v.iter().enumerate()"#),
        "bytes(N) keys render the JSON array form"
    );

    // --- nullable ---------------------------------------------------------
    // A nullable field matches on the `Option`, never on a `Value`. Matching a
    // reference is what lets one emitted form serve the owning record side and the
    // borrowing probe side (`Option<String>` vs `Option<&str>`).
    assert!(
        flat.contains(r"Some(__v)=>{letmut__k=String::with_capacity(1+__v.len());"),
        "nullable string keys match the Option and reuse the non-nullable body"
    );
    assert!(
        flat.contains(r"None=>String::from('\u{0}'),"),
        "the None arm keys into the null bucket"
    );
    // An optional FK (`?Owner`) is `Option<Uuid>` in Rust but carries its
    // optionality in the RelationType, NOT a `Nullable` wrapper. Missing that is a
    // silent mis-key, so assert the unlinked case is handled as an Option.
    assert!(
        flat.contains(r"match&(record.editor){Some(__v)=>{letmut__buf=[0u8;36];"),
        "an optional FK keys through the Option arm, not as a bare Uuid (#230)"
    );
}

/// #235: `@length` takes named `min:`/`max:` arguments, and single-arg `@length(N)`
/// now means an EXACT length.
///
/// Five spellings, five distinct emitted checks. The single-arg change is the one
/// that matters most here: it used to emit `> N` and now emits `!= N`, a narrowing
/// that no other layer would catch — the schema still parses and the crate still
/// compiles, so this assertion and the parser's warning are the whole safety net.
#[test]
fn test_rust_generation_length_named_args() {
    let src = r#"
Doc {
  id: +uuid
  floor: string @length(min: 3)
  ceiling: string @length(max: 20)
  both: string @length(min: 3, max: 64)
  positional: string @length(3, 5)
  exact: string @length(7)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    // min only — a floor with no ceiling, which was inexpressible before #235.
    assert!(
        flat.contains("if__v.chars().count()<3i64asusize"),
        "`@length(min: 3)` emits a floor check.\nGot: {code}"
    );
    assert!(
        !flat.contains(r#""floor",message:"lengthmustbe<=3""#),
        "`@length(min: 3)` must NOT be read as a maximum"
    );

    // max only — the new spelling for what `@length(20)` used to mean.
    assert!(
        flat.contains("if__v.chars().count()>20i64asusize"),
        "`@length(max: 20)` emits a ceiling check"
    );

    // Both, named and positional, emit the same range check.
    assert!(
        flat.contains("__len<3i64asusize||__len>64i64asusize"),
        "`@length(min: 3, max: 64)` emits a range check"
    );
    assert!(
        flat.contains("__len<3i64asusize||__len>5i64asusize"),
        "`@length(3, 5)` is unchanged — still min, max"
    );

    // The breaking one: exactly N, not at most N.
    assert!(
        flat.contains("if__v.chars().count()!=7i64asusize"),
        "`@length(7)` now emits an EQUALITY check (#235)"
    );
    assert!(
        !flat.contains("if__v.chars().count()>7i64asusize"),
        "`@length(7)` must no longer emit the old `> 7` maximum check"
    );

    // The messages are what a 422 body shows, so they have to say which rule
    // rejected the value rather than all reading "length must be ...".
    for msg in [
        "lengthmustbe>=3",
        "lengthmustbe<=20",
        "lengthmustbebetween3and64",
        "lengthmustbebetween3and5",
        "lengthmustbeexactly7",
    ] {
        assert!(
            flat.contains(msg),
            "each spelling reports its own rule — missing {msg}"
        );
    }
}

/// #243: an array past serde's `[T; N]` ceiling (N = 32) must still generate code
/// that compiles.
///
/// serde implements `Serialize`/`Deserialize` for arrays only up to N = 32, so a
/// `bytes(64)` or `[u32; 40]` field made the `#[derive(Serialize, Deserialize)]` on
/// the generated type fail to resolve — an entire crate that did not compile. It was
/// never only an *index* problem: a plain, unindexed field broke it just the same,
/// because the derive is on the struct.
///
/// Three shapes reach it and nothing else can, because nested fixed arrays do not
/// parse. Each gets its own helper, and an inline `struct` is a second emission site
/// that broke independently of the model one.
///
/// This test pins the two halves that could silently regress: that oversized fields
/// get the attribute, and that fields serde can already handle do **not** — crossing
/// the boundary must not rewrite output for every existing schema.
///
/// That the result *compiles* is proven by `tests/oversized_array_test.rs`, which
/// builds a generated crate carrying every shape; a string assertion cannot.
#[test]
fn test_rust_generation_oversized_bytes_serde() {
    let src = r#"
struct Fp {
  digest: bytes(64)
  small: bytes(8)
  wide: [u32; 40]
}

Doc {
  id: +uuid
  plain: bytes(64)
  fingerprint: ^bytes(64)
  opt_hash: bytes(48)?
  boundary: bytes(32)
  past: bytes(33)
  small: ^bytes(8)
  fp: Fp
  arr_big: [bytes(64); 2]
  arr_small: [bytes(8); 2]
  many: [u32; 40]
  few: [u32; 4]
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    for module in ["mod__forgedb_big_bytes", "mod__forgedb_big_array"] {
        assert!(
            flat.contains(module),
            "`{module}` must be emitted when a field needs it.\nGot: {code}"
        );
    }

    // Past the ceiling: each shape points at its own helper.
    for (field, path) in [
        ("plain", "__forgedb_big_bytes"),
        ("fingerprint", "__forgedb_big_bytes"),
        ("past", "__forgedb_big_bytes"),
        ("opt_hash", "__forgedb_big_bytes::option"),
        ("arr_big", "__forgedb_big_bytes::array"),
        ("many", "__forgedb_big_array"),
        // The inline struct: a second field-emission site (`generate_struct`), which
        // carries the same derive and so broke the same way.
        ("digest", "__forgedb_big_bytes"),
        ("wide", "__forgedb_big_array"),
    ] {
        let decl = format!(r#"#[serde(with="{path}")]pub{field}:"#);
        assert!(
            flat.contains(&decl),
            "`{field}` is past serde's array ceiling and must use `{path}`"
        );
    }
    // utoipa stops at 32 for the same reason serde does, so the schema type is
    // declared rather than derived.
    for schema_ty in [
        "#[schema(value_type=Vec<u8>)]",
        "#[schema(value_type=Vec<Vec<u8>>)]",
        "#[schema(value_type=Option<Vec<u8>>)]",
    ] {
        assert!(
            flat.contains(schema_ty),
            "utoipa cannot describe the oversized array either — expected {schema_ty}"
        );
    }

    // At or under the ceiling: serde's own impl applies and nothing changes. This is
    // the half that keeps the fix from churning every existing schema's output.
    // Asserted as "no attribute at all", by pinning the field to the end of the
    // preceding one — a window-of-N-chars search would just find the *previous*
    // field's attribute.
    for field in ["boundary", "small", "few", "arr_small"] {
        let decl = format!("pub{field}:[");
        let at = flat
            .find(&decl)
            .unwrap_or_else(|| panic!("`{field}` must exist"));
        let preceding = flat[..at].chars().next_back().unwrap();
        assert!(
            matches!(preceding, ',' | '{'),
            "`{field}` is within serde's array ceiling and must carry no attribute, \
             but is preceded by {preceding:?}"
        );
    }
}

/// #243: the helper modules are emitted ONLY when a field needs them.
///
/// Generated code is tailored to the schema — a schema that never crosses the
/// ceiling must not carry the machinery, and its output must stay byte-identical to
/// what it produced before the fix. This is what kept every existing snapshot from
/// being rewritten.
#[test]
fn test_rust_generation_no_big_array_serde_when_unneeded() {
    let src = r#"
struct Fp {
  small: bytes(8)
}

Doc {
  id: +uuid
  code: ^bytes(32)
  arr: [bytes(32); 32]
  nums: [u32; 32]
  fp: Fp
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // Exactly at the ceiling in every position — serde covers all of it.
    assert!(
        !code.contains("__forgedb_big_bytes") && !code.contains("__forgedb_big_array"),
        "a schema that stays within serde's array ceiling must carry no helper.\nGot: {code}"
    );
}

/// #242: the f64 total-order key helper is emitted on demand, like the oversized
/// array serde above. Generated code is tailored to its schema — a schema that
/// indexes no `f64` must not carry the machinery, and its output must be
/// byte-identical to what it produced before #242.
#[test]
fn test_rust_generation_no_f64_key_when_unneeded() {
    let src = r#"
Reading {
  id: +uuid
  label: ^string
  raw: f64
  pair: f64?
  count: ^u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // `raw` and `pair` are f64 but UNINDEXED, so no key is ever derived for them.
    // Gating on "the schema mentions f64" rather than "the schema indexes f64"
    // would emit a dead helper here.
    assert!(
        !code.contains("__forgedb_f64_key"),
        "an unindexed f64 needs no key helper.\nGot: {code}"
    );
}

/// #242: the gate must count composite components too. A composite key is built
/// from each component's `index_key_expr`, so an f64 component reaches the same
/// encoding — missing it would emit a call to a function that was never generated,
/// and the generated crate would not compile.
#[test]
fn test_rust_generation_f64_key_emitted_for_a_composite_component_only() {
    let src = r#"
Sample {
  id: +uuid
  bucket: ^string
  score: f64

  @index(bucket, score)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // `score` carries no `^` of its own — its only index membership is the
    // composite.
    assert!(
        code.contains("fn __forgedb_f64_key"),
        "an f64 reachable only as a composite component still needs the helper.\nGot: {code}"
    );
}

/// #233: `char(N)` is a *spelling*, not a distinct type — the deprecated form must
/// emit byte-identical code across every generator, in every type position.
///
/// This is the guarantee that makes the deprecation safe to ship: a user who does
/// nothing gets exactly the database they had, and a user who runs the suggested
/// fix gets exactly the same one. It cannot be a snapshot test, because the point
/// is the *relationship* between two schemas rather than either one's content.
#[test]
fn test_generation_char_and_bytes_are_byte_identical() {
    // Every position `parse_type` handles separately: bare, postfix-nullable,
    // prefix-nullable, inside a fixed array, and behind index/unique modifiers.
    let deprecated = r#"
Thing {
  id: +uuid
  code: char(3)
  opt: char(2)?
  pre: ?char(4)
  arr: [char(8); 2]
  key: ^&char(5)
  name: string
}
"#;
    let canonical = deprecated.replace("char(", "bytes(");

    let parse = |src: &str| {
        let mut parser = forgedb_parser::Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        (schema, parser.take_warnings())
    };

    let (dep_schema, dep_warnings) = parse(deprecated);
    let (can_schema, can_warnings) = parse(&canonical);

    assert_eq!(
        dep_warnings.len(),
        5,
        "one deprecation per `char` occurrence, including inside `[...]` and behind `^&`"
    );
    assert!(
        can_warnings.is_empty(),
        "the canonical spelling is silent: {can_warnings:?}"
    );

    // Every generator, not just the Rust one — the rename touches the type mapping
    // in each of them, so each is a place the two spellings could diverge.
    assert_eq!(
        RustGenerator::generate(&dep_schema).unwrap().code,
        RustGenerator::generate(&can_schema).unwrap().code,
        "database.rs"
    );
    assert_eq!(
        ApiGenerator::generate(&dep_schema).unwrap().code,
        ApiGenerator::generate(&can_schema).unwrap().code,
        "api.rs"
    );
    assert_eq!(
        TypeScriptGenerator::generate(&dep_schema).unwrap().code,
        TypeScriptGenerator::generate(&can_schema).unwrap().code,
        "types.ts"
    );
}

/// #250: every `<Model>ScanRef<'a>` must actually use its lifetime, or the emitted
/// crate does not compile (`error[E0392]: lifetime parameter 'a is never used`).
///
/// The lifetime exists so `string` scan fields can borrow out of the buffered
/// column span. A model whose every scannable field is fixed-size has nothing to
/// attach it to — and that is not an exotic shape: it is the ordinary pure
/// join/link row (identity, timestamps, foreign keys, no string), which is what
/// `Star` is in `examples/code-hosting`.
///
/// The defect shipped from #160/#224 because the snapshot tests compare generated
/// code as *strings* and never compile it, and every schema used in a manual
/// compile-check happened to give each model a string field. So the durable guard
/// is not "assert the struct looks right" — it is **a fixture that carries a
/// string-free model at all**, which is what `Link` is below.
#[test]
fn test_rust_generation_scan_ref_lifetime_is_always_anchored() {
    let schema_src = r#"
Owner {
  id: +uuid
  name: string
}

Link {
  id: +uuid
  created_at: +timestamp
  owner: *Owner
  weight: u32
}
"#;
    let schema = forgedb_parser::Parser::new(schema_src)
        .unwrap()
        .parse()
        .unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    // `Link` has no `string`, so nothing in its view borrows: the anchor carries
    // the lifetime.
    let link = extract_struct(&code, "pub struct LinkScanRef<'a>");
    assert!(
        link.contains("PhantomData<&'a ()>"),
        "a scan view with no borrowing field must anchor 'a, else E0392:\n{link}"
    );

    // `Owner.name` borrows, so the anchor would be redundant — and emitting it
    // unconditionally would churn every existing snapshot. Pin that it does not.
    let owner = extract_struct(&code, "pub struct OwnerScanRef<'a>");
    assert!(
        owner.contains("&'a str"),
        "expected the borrowed string field:\n{owner}"
    );
    assert!(
        !owner.contains("PhantomData"),
        "a view that already borrows needs no anchor:\n{owner}"
    );
}

/// Slice out a function body by its signature prefix, brace-balanced.
///
/// Unlike [`extract_struct`], a method body nests braces freely, so stopping at
/// the first `}` would cut it off inside the first block and quietly make any
/// ordering assertion vacuous.
fn extract_fn<'a>(code: &'a str, sig: &str) -> &'a str {
    let start = code
        .find(sig)
        .unwrap_or_else(|| panic!("`{sig}` not found in generated code"));
    let rest = &code[start..];
    let open = rest.find('{').expect("function has no body");
    let mut depth = 0usize;
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated function body for `{sig}`");
}

/// Slice out a struct body by its declaration line, for the assertions above.
fn extract_struct<'a>(code: &'a str, decl: &str) -> &'a str {
    let start = code
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found in generated code"));
    let rest = &code[start..];
    let end = rest.find('}').expect("unterminated struct") + 1;
    &rest[..end]
}

/// #250: the anchor's name is derived, not fixed, because `.forge` field names are
/// only required to be snake_case — `__borrow: u32` parses and validates today.
/// On a string-free model a hardcoded anchor would land beside it and emit the
/// field twice (`error[E0124]: field `__borrow` is already declared`).
#[test]
fn test_rust_generation_scan_ref_anchor_avoids_a_field_name_collision() {
    let schema = forgedb_parser::Parser::new(
        r#"
Collide {
  id: +uuid
  __borrow: u32
  __borrow_: u32
}
"#,
    )
    .unwrap()
    .parse()
    .unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let view = extract_struct(&code, "pub struct CollideScanRef<'a>");

    // Both obvious names are taken, so the anchor steps past them.
    assert!(
        view.contains("pub __borrow__: ::std::marker::PhantomData<&'a ()>"),
        "anchor should skip every taken name:\n{view}"
    );
    assert_eq!(
        view.matches("__borrow:").count(),
        1,
        "the user's own field must appear exactly once:\n{view}"
    );
}

// ---- #187: `+u32` / `+u64` auto-increment ---------------------------------
//
// The design (RFC #187, Gates 1+2 accepted): one monotonic in-memory counter per
// auto-integer field, seeded by the scans that already run, floored by a
// high-water mark persisted in `Manifest.auto_sequences`. `0` is the allocate
// sentinel; an explicitly supplied value advances the counter via `fetch_max`;
// a rolled-back transaction burns its number (monotonic and unique, not
// contiguous).
//
// Assertions here are identifier-precise (`mentions_ident`) per the #258 lesson:
// a bare `contains` cannot tell `__autoseq_id` from `__autoseq_ident`.
//
// The counter is `__autoseq_<field>`, NOT `__seq_<field>`: the generated Tier-2 /
// Tier-3 commit path already binds locals named `__seq`, `__seq_arc` and
// `__seq_start`, so a model with a field named `arc` or `start` would emit a
// counter whose name reads as one of those. Nothing would fail to compile — it
// would just be indistinguishable in the emitted source, including to the
// "emits no counter" guard below.

/// The counter must be allocated from at **all three** create surfaces —
/// `Database::create_*`, `TxHandle::create_*`, and `ConcurrentTxHandle::create_*`.
/// The third is the one that would be missed: it is generated in a separate
/// function from the other two, and its prepare closure runs with no write lock,
/// which is exactly the path where a missing allocation silently yields `0`.
#[test]
fn test_rust_generation_integer_auto_allocates_at_every_create_surface() {
    let src = r#"
Ticket {
  id: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        mentions_ident(&code, "__autoseq_id"),
        "an integer auto gets a counter field on the storage struct (#187)"
    );
    assert_eq!(
        code.matches("__alloc_id()").count(),
        3,
        "all three create surfaces allocate — Database, TxHandle, and \
         ConcurrentTxHandle (#187)"
    );
}

/// A *non-identity* integer auto carrying `&` puts allocation and the #258
/// uniqueness enforcement on the same field, so they must coexist. Guarded together
/// because the two features touch adjacent generated code and either could
/// plausibly be written to replace the other. (#187 once *required* the `&` here;
/// since #260 it is a choice, which makes the coexistence no less load-bearing.)
#[test]
fn test_rust_generation_non_identity_integer_auto_allocates_and_stays_unique() {
    let src = r#"
Invoice {
  id: +uuid
  number: &+u64
  total: f64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        mentions_ident(&code, "__autoseq_number"),
        "a non-identity integer auto gets its own counter (#187)"
    );
    assert_eq!(
        code.matches("__alloc_number()").count(),
        3,
        "allocated at every create surface, identity or not (#187)"
    );
    // #258: `&` on a non-identity auto still builds the index and enforces it.
    assert!(
        mentions_ident(&code, "number_index"),
        "`&` still builds its index (#258 must not regress under #187)"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("ValidationError::Unique{model:\"Invoice\",field:\"number\""),
        "`&` still enforces uniqueness (#258 must not regress under #187)"
    );
    // The identity is a uuid here, so it must NOT have grown a counter.
    assert!(
        !mentions_ident(&code, "__autoseq_id"),
        "a `+uuid` identity allocates from randomness, not a counter (#187)"
    );
}

/// The counter is writer state. A `*StorageReader` is a read-only snapshot handle
/// (#56-B) and must not carry one — a reader holding a counter would read as a
/// second allocator and invites a future change to allocate through it.
#[test]
fn test_rust_generation_reader_carries_no_sequence_counter() {
    let src = r#"
Ticket {
  id: +u64
  title: ^string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    let reader = extract_struct(&code, "pub struct TicketStorageReader");
    assert!(
        !reader.contains("__autoseq_"),
        "a read-only reader must carry no allocator state (#187):\n{reader}"
    );
    // Control: the writer does carry it, so the assertion above is not vacuous.
    let storage = extract_struct(&code, "pub struct TicketStorage");
    assert!(
        storage.contains("__autoseq_id"),
        "the writer is where the counter lives (#187):\n{storage}"
    );
}

/// The floor must reach disk **before** the destructive rewrite, not only after.
///
/// `compact_model_keeping` physically drops the dead rows, and for every value
/// allocated since process open those rows are the only remaining record that the
/// value was issued (the manifest is written at open and at compaction, never in
/// between). Persist only afterwards and a crash in that window leaves the reopen
/// scan deriving a lower maximum and re-issuing the difference — the one case a
/// rescan cannot recover from, which is the entire reason the floor exists.
///
/// `tests/auto_increment_test.rs` proves the behaviour by running it; this pins
/// the *ordering* in the emitted source, because the two orderings produce an
/// identical final state on the success path and diverge only across a crash.
#[test]
fn test_rust_generation_autoseq_floor_persists_before_compaction() {
    let src = r#"
Ticket {
  id: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    let compact = extract_fn(&code, "pub fn compact");
    let write = compact
        .find("write_manifest")
        .expect("compact() must persist the floor");
    let destroy = compact
        .find("compact_model_keeping")
        .expect("compact() must call the byte GC");
    assert!(
        write < destroy,
        "the auto-increment floor must be persisted BEFORE compact_model_keeping \
         destroys the rows that are its only other evidence (#187):\n{compact}"
    );
    // And the failure is a refusal, not a warning: compaction is always safe to
    // retry later, whereas re-issuing an id escapes through the replication log,
    // backups, and any URL holding the value.
    assert!(
        compact[..destroy].contains("return;"),
        "a floor that cannot be persisted must ABORT the compaction, not proceed \
         (#187):\n{compact}"
    );
}

/// #260: a **bare** integer auto — neither the model's identity nor `&unique` —
/// claims its allocated value in the opaque write-set via a third key class.
///
/// This is the entire feature. Without the claim, two coordinated writers that
/// derive the same next value both commit and neither notices, which is why #187
/// refused the shape outright rather than shipping it unprotected.
///
/// All three write-set builders must emit it. They are separate code paths —
/// `TxHandle::__write_set` (`db.transaction()`), `transaction_concurrent`, and
/// `transaction_coordinated` — and missing one leaves a silent hole on that path
/// alone, which no single-path test would catch.
#[test]
fn test_rust_generation_bare_integer_auto_claims_its_value() {
    let src = r#"
Ticket {
  id: +uuid
  seq: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("staged_sequence_keys"),
        "a bare integer auto needs a staging buffer for its claims (#260)"
    );
    // The claim must be keyed off the record's final value, so it covers an
    // explicitly-supplied value too (#187 decision 5 lets one through, and an
    // explicit 7 racing an allocated 7 is the same collision).
    assert!(
        code.contains("staged_sequence_keys.insert"),
        "the allocated value must actually be staged, not just buffered (#260)"
    );

    let claims = code.matches("b\"s\"").count();
    assert!(
        claims >= 3,
        "all THREE write-set builders must emit the sequence claim — \
         __write_set, transaction_concurrent, transaction_coordinated — \
         found {claims} (#260)"
    );
    assert!(
        extract_fn(&code, "pub fn __write_set").contains("b\"s\""),
        "the Tier-1/2 db.transaction() path must claim (#260)"
    );
}

/// The zero-churn guard for #260: an integer auto that is already conflict-visible
/// — the identity, or `&unique` — must emit **no** sequence-claim machinery.
///
/// Load-bearing for two reasons. It keeps the new key class off every schema that
/// was legal before #260 (the identity contributes a row key and `&unique` a
/// unique-claim key, so a third claim would be redundant work on every insert).
/// And because the staging buffer lives on the schema-level `TxHandle` /
/// `ConcurrentTxHandle` — not per model — an ungated field would appear in every
/// generated crate and diff every snapshot in this file for a feature the schema
/// does not use.
#[test]
fn test_rust_generation_conflict_visible_autos_emit_no_sequence_claim() {
    for src in [
        // identity integer auto — already contributes the row key
        "Ticket {\n  id: +u64\n  title: string\n}\n",
        // non-identity but unique — already contributes the unique-claim key
        "Ticket {\n  id: +uuid\n  seq: &+u64\n  title: string\n}\n",
    ] {
        let mut parser = forgedb_parser::Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        let code = RustGenerator::generate(&schema).unwrap().code;

        assert!(
            !code.contains("staged_sequence_keys"),
            "{src:?} is already conflict-visible; a sequence claim would be dead \
             weight, and the field would churn every snapshot (#260)"
        );
        assert!(
            !code.contains("b\"s\""),
            "{src:?} must emit no sequence claim key (#260)"
        );
    }
}

/// The no-regression guard for the whole existing corpus: a model whose only autos
/// are `+uuid` / `+timestamp` must emit **no** counter machinery at all. Every
/// schema in `examples/` except `iot-sensors` is this shape, so a leak here would
/// change output repo-wide.
#[test]
fn test_rust_generation_non_integer_autos_emit_no_counter() {
    let src = r#"
Event {
  id: +uuid
  created_at: +timestamp
  ref_id: &+uuid
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        !code.contains("__autoseq_"),
        "no counter field for a model with no integer auto (#187)"
    );
    assert!(
        !code.contains("__alloc_"),
        "no allocation call for a model with no integer auto (#187)"
    );
    // NOT asserted: the absence of `SequenceExhausted`. `ValidationError` is a
    // fixed-shape enum emitted once per schema — every variant is always present,
    // whether or not any field can produce it. What must be absent is the
    // machinery above that would *raise* it.
}

/// The M2M junction writes its own `Manifest` literal, separate from the model
/// one, and it is compile-forced by `Manifest` having no `Default` — so it must be
/// updated in the same pass. Pinned because "it compiles" is the only feedback that
/// site otherwise gives.
///
/// The fixture keeps a third model because the assertion needs an integer auto
/// *somewhere*: the junction itself has no fields of its own — only the two
/// endpoint ids — so its sequence map is empty no matter what the endpoints are
/// keyed on (#266 widened `valid_m2m` past the old uuid-PK-only gate, so the
/// endpoints may now carry integer autos themselves). `Ticket` supplies a model
/// that does allocate, which is what makes the junction's empty map a real
/// assertion rather than a description of a schema where nothing allocates
/// anywhere.
#[test]
fn test_rust_generation_junction_manifest_carries_empty_sequence_map() {
    let src = r#"
Student {
  id: +uuid
  courses: [Course]
}

Course {
  id: +uuid
  students: [Student]
}

Ticket {
  id: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("auto_sequences:Default::default()")
            || flat.contains("auto_sequences:std::collections::BTreeMap::new()"),
        "the junction manifest takes an empty sequence map — a junction has no \
         fields, so it can have no auto-integer field (#187)"
    );
    // `Ticket` DOES carry a real map, so the emptiness above is specific to the
    // junction rather than the feature quietly emitting nothing anywhere.
    assert!(
        mentions_ident(&code, "__autoseq_id"),
        "a model with an integer auto still allocates alongside the junction (#187)"
    );
}

/// The create contract widens: an integer auto becomes server-synthesized, so it
/// gains `#[serde(default)]` and drops out of the create input — the same
/// treatment `+uuid` already gets. This is what stops a REST create that omits an
/// integer id from 422-ing.
///
/// Note this fixes the #188 *class* only for the generators that ask
/// `is_server_synthesized`. The TS SDK and the OpenAPI `required` list compute the
/// create shape by guessing and stay wrong until #259 — deliberately out of scope
/// here, and asserted nowhere in this test.
#[test]
fn test_rust_generation_integer_auto_is_server_synthesized() {
    let src = r#"
Ticket {
  id: +u64
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    let model = extract_struct(&code, "pub struct Ticket");
    assert!(
        model.contains("#[serde(default)]"),
        "an integer auto may be omitted from a create body, so it needs a serde \
         default exactly as `+uuid` does (#187/#188):\n{model}"
    );
}

/// Overflow is an error, never a wrap. A `+u32` that wraps past `u32::MAX` would
/// re-issue `0` — which is also the allocate sentinel — and then collide with
/// every id already handed out.
#[test]
fn test_rust_generation_integer_auto_guards_overflow() {
    let src = r#"
Small {
  id: +u32
  name: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        mentions_ident(&code, "SequenceExhausted"),
        "a `+u32` counter must refuse to wrap (#187)"
    );
    assert!(
        code.contains("u32::MAX"),
        "the guard is against the field's own width, not u64's (#187)"
    );
}

// ===========================================================================
// #238 — `string(N)` / `string(N!)`: fixed-width inline string columns.
//
// These are LAYOUT guards, and they exist because every failure mode in this
// area produces working, wrong code rather than a build error. `rust.rs` has
// three fall-throughs a missing `StringN` arm would land in — `_ => 8` for the
// column width, `_ => String` for the field type, and a `FixedBytes(size)` that
// is only as right as the width arm above it. An 8-byte stride silently
// truncates every value and the generated crate compiles clean.
//
// So each of these asserts the emitted *width*, not merely that it generated.
// ===========================================================================

/// Generate `database.rs` for a one-model schema.
fn db_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    RustGenerator::generate(&schema).unwrap().code
}

/// The `FixedColumn::new(...)` initializer for one field's column, whitespace
/// collapsed so assertions track the shape rather than prettyplease's wrapping.
fn column_init(code: &str, field: &str) -> String {
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let needle = format!("{field}_col: FixedColumn::new(");
    let at = flat.find(&needle).unwrap_or_else(|| {
        panic!("no FixedColumn init for `{field}` — it did not get a fixed column at all")
    });
    let tail = &flat[at..];
    tail[..tail.find(") .expect").unwrap_or(tail.len().min(200))].to_string()
}

/// Res 6: the exact form is exactly N bytes, with no length prefix — the
/// narrowest of the three shapes, and byte-identical in construction to
/// `bytes(N)`.
#[test]
fn test_rust_generation_string_n_exact_stride_is_n() {
    let code = db_for("Doc {\n  id: +uuid\n  currency: string(3!)\n}\n");
    let init = column_init(&code, "currency");
    assert!(
        init.contains("3usize"),
        "`string(3!)` is a 3-byte slot, not the `_ => 8` fall-through: {init}"
    );
    assert!(!init.contains("8usize"), "a fall-through width would be 8: {init}");
}

/// Res 6's derived prefix width, asserted from both sides of the 1→2 byte
/// boundary. A flat one-byte prefix cannot work under `@utf8`: 64 characters at
/// four bytes each is 256, which does not fit in a byte.
#[test]
fn test_rust_generation_string_n_slot_widths() {
    for (decl, field, want) in [
        // default alphabet: one byte per character, one prefix byte
        ("string(32)", "sku", 33usize),
        ("string(255)", "wide", 256),
        ("string(26!)", "key", 26),
        // @utf8: four bytes per character; prefix crosses to two bytes when the
        // payload passes 255, i.e. at N = 64
        ("string(63) @utf8", "just_under", 63 * 4 + 1),
        ("string(64) @utf8", "just_over", 64 * 4 + 2),
        // the exact form still carries a prefix under @utf8 — exactly N
        // *characters* is still a variable N..4N *bytes*
        ("string(4!) @utf8", "quad", 4 * 4 + 1),
    ] {
        let src = format!("Doc {{\n  id: +uuid\n  {field}: {decl}\n}}\n");
        let init = column_init(&db_for(&src), field);
        assert!(
            init.contains(&format!("{want}usize")),
            "`{decl}` must emit a {want}-byte slot, got: {init}"
        );
    }
}

/// A nullable inline string prepends a one-byte presence tag, so `None` and
/// `Some("")` round-trip distinctly — the same encoding every other nullable
/// column on this path uses.
#[test]
fn test_rust_generation_string_n_nullable_adds_a_presence_byte() {
    let code = db_for("Doc {\n  id: +uuid\n  note: string(10)?\n}\n");
    let init = column_init(&code, "note");
    assert!(
        init.contains("12usize"),
        "`string(10)?` is 1 (present) + 10 (payload) + 1 (prefix) = 12: {init}"
    );
}

/// The finding-2 guard. `string(N)` must NOT be classified as a variable column;
/// if it were, every other scenario here would still pass and the type would
/// have silently become the thing it was invented to replace.
#[test]
fn test_rust_generation_string_n_is_not_a_variable_column() {
    let code = db_for("Doc {\n  id: +uuid\n  sku: string(32)\n  body: string\n}\n");
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("sku_col: FixedColumn"),
        "`string(32)` gets a FixedColumn"
    );
    assert!(
        !flat.contains("sku_col: VariableColumn"),
        "`string(32)` must never get a VariableColumn"
    );
    // The bare `string` beside it is untouched (res 10).
    assert!(
        flat.contains("body_col: VariableColumn"),
        "bare `string` still gets a VariableColumn"
    );
    // ...and it does not ride the variable codec either.
    assert!(
        !flat.contains("self.sku_col.append_string"),
        "`string(32)` must not be appended through the variable-string codec"
    );
}

/// The manifest records a *physical* width. `ColumnType::String` would claim the
/// variable layout and mislead every schema-blind reader (backup, inspector).
#[test]
fn test_rust_generation_string_n_manifest_is_fixed_bytes() {
    let code = db_for("Doc {\n  id: +uuid\n  sku: string(32)\n}\n");
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = flat.find("\"sku\"").expect("sku appears in the manifest writer");
    let window = &flat[at..(at + 300).min(flat.len())];
    assert!(
        window.contains("ColumnType::FixedBytes(33usize)"),
        "manifest column type is FixedBytes(33): {window}"
    );
}

/// The finding-1 guard, at the codegen layer: the scan decodes a `string(N)` by
/// BORROWING the slot, never by allocating a `Vec` per row. `read_bytes` is what
/// every other fixed column uses, and it is what this must not use — the
/// difference is invisible to a snapshot and inverts the issue's entire result.
#[test]
fn test_rust_generation_string_n_scan_borrows_the_slot() {
    let code = db_for("Doc {\n  id: +uuid\n  key: string(26!)\n  sku: string(32)\n}\n");
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    // The exact form: the slot IS the value, so the substrate can do the whole
    // read (`read_str`), no prefix decode at all.
    assert!(
        flat.contains("key_col .read_str"),
        "`string(26!)` reads the whole slot as UTF-8: {flat}"
    );
    // The at-most form: borrow the slot, decode the prefix in generated code.
    assert!(
        flat.contains("sku_col .read_slice"),
        "`string(32)` borrows the slot rather than copying it"
    );

    // The borrow is what the SCAN does. `get`/`all` still materialize a `String`
    // and read through `read_bytes` like every other fixed column — a borrow is
    // not available there, because the live column reads through the file on
    // every access and has nothing to lend. So scope the absence to the scan
    // scope's body rather than to the whole file.
    let scan = &code[code.find("pub fn __with_scan").expect("the scan scope is emitted")..];
    let scan: String = scan.split_whitespace().collect::<Vec<_>>().join(" ");
    let scan = &scan[..scan.find("pub fn ").map(|i| i + 7).unwrap_or(scan.len())];
    assert!(
        !scan.contains("read_bytes"),
        "a per-row `Vec` on the scan path is the one outcome #238 exists to avoid: {scan}"
    );
}

/// Res 6, asserted as an *absence*: the exact form has no prefix to decode. A
/// present-but-unused prefix is merely slower rather than wrong, so nothing else
/// here would catch it.
#[test]
fn test_rust_generation_string_n_exact_has_no_prefix_decode() {
    let exact = db_for("Doc {\n  id: +uuid\n  key: string(26!)\n}\n");
    let inexact = db_for("Doc {\n  id: +uuid\n  key: string(26)\n}\n");
    assert!(
        inexact.contains("__forgedb_inline_len"),
        "the at-most form decodes a length prefix"
    );
    assert!(
        !exact.contains("__forgedb_inline_len"),
        "the exact form has no prefix, so there is nothing to decode"
    );
}

/// Two properties of the packed slot that every other assertion here is blind
/// to, because both are about the *bytes written* rather than the shape of the
/// code. Added after a mutation run: breaking either one left the whole suite
/// green.
///
///  1. The length prefix records a **byte** count. A character count reads back
///     a truncated value for any multi-byte `@utf8` field, and recovering the
///     byte end from a character count would mean walking the UTF-8 on every
///     row — the per-row cost #238 exists to avoid.
///  2. The slot's unused tail is **zero**. Nothing correctness-critical needs
///     it (the prefix bounds the read), but it is what makes a column file
///     byte-reproducible for the same logical content, which is what lets a
///     diff of two data dirs mean anything.
#[test]
fn test_rust_generation_string_n_prefix_is_bytes_and_the_tail_is_zeroed() {
    let code = db_for("Doc {\n  id: +uuid\n  sku: string(32)\n  tag: string(8) @utf8\n}\n");
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        flat.contains("let __n = __b.len().min("),
        "the prefix is written from the value's BYTE length: {flat}"
    );
    assert!(
        !flat.contains("chars().count().min("),
        "a character count in the prefix truncates every multi-byte value"
    );
    assert!(
        flat.contains("let mut __buf = [0u8;"),
        "the packing buffer is zero-filled, so the unused tail is deterministic"
    );
}

/// Res 4: the ASCII restriction is a *value* constraint — enforced on every
/// write, like `@pattern`, not a validation-time-only check. And it runs FIRST,
/// because it is the only check that can make a later one report a cause that is
/// not the real one (a `@pattern` failure over a non-ASCII value).
#[test]
fn test_rust_generation_string_n_ascii_check_runs_before_the_directives() {
    let code = db_for(
        "Doc {\n  id: +uuid\n  code: string(8) @pattern(\"^[a-z]+$\") @length(min: 2)\n}\n",
    );
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let ascii = flat.find("rule: \"ascii\"").expect("an ascii rule is emitted");
    let pattern = flat.find("rule: \"pattern\"").expect("the @pattern check survives");
    let length = flat.find("rule: \"length\"").expect("the @length check survives");
    assert!(ascii < pattern, "ASCII is checked before @pattern");
    assert!(ascii < length, "ASCII is checked before @length");
    // ...and the message points at the way out.
    assert!(flat.contains("@utf8"), "the ascii diagnostic names the opt-in");
}

/// `@utf8` removes the ASCII check — that is the whole of what it does to the
/// write path.
#[test]
fn test_rust_generation_utf8_drops_the_ascii_check() {
    let code = db_for("Doc {\n  id: +uuid\n  title: string(8) @utf8\n}\n");
    assert!(
        !code.contains("\"ascii\""),
        "@utf8 opts out of the one-byte-per-character alphabet"
    );
}

/// Res 1 + res 2: the width bound is enforced at write, in characters, and the
/// exact form rejects a short value as well as a long one.
#[test]
fn test_rust_generation_string_n_enforces_the_character_bound() {
    let at_most = db_for("Doc {\n  id: +uuid\n  sku: string(32)\n}\n");
    let flat: String = at_most.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("chars().count()"),
        "the bound counts characters, not bytes (res 3)"
    );
    assert!(flat.contains("rule: \"string_n\""), "a width violation is its own rule");

    let exact = db_for("Doc {\n  id: +uuid\n  key: string(26!)\n}\n");
    let eflat: String = exact.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        eflat.contains("!= 26usize"),
        "the exact form rejects any length but N: {eflat}"
    );
}

/// `string(N)` is `Ord`, so it joins the same filterable/sortable class as bare
/// `string`, and its index key is the `\u{1}` string class — the SAME key the
/// same content would produce in a bare `string` column, so widening a column
/// later keeps its index semantics.
#[test]
fn test_api_generation_string_n_is_filterable_and_indexed_as_a_string() {
    let src = "Doc {\n  id: +uuid\n  sku: ^string(32)\n}\n";
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let db_code = RustGenerator::generate(&schema).unwrap().code;
    let api_code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(db_code.contains("sku_index"), "an `^string(N)` field is indexed");
    assert!(
        db_code.contains("find_by_sku"),
        "and gets the generated probe"
    );
    assert!(
        db_code.contains("'\\u{1}'"),
        "the index key is the string class, like a bare `string`"
    );
    let api_flat: String = api_code.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        api_flat.contains("\"sku\""),
        "the REST filter admits the column"
    );
}

/// The wire is a string, everywhere. The fixed slot is a storage fact; a client
/// must not be able to tell.
#[test]
fn test_generation_string_n_is_a_string_on_every_wire() {
    let src = "Doc {\n  id: +uuid\n  sku: string(32)\n  key: string(26!)\n}\n";
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();

    let ts = TypeScriptGenerator::generate(&schema).unwrap().code;
    assert!(ts.contains("sku: string"), "TS: a string\n{ts}");

    let openapi = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&openapi).unwrap();
    let props = &spec["components"]["schemas"]["Doc"]["properties"];
    assert_eq!(props["sku"]["type"], "string");
    assert_eq!(props["sku"]["maxLength"], 32);
    assert!(props["sku"].get("minLength").is_none(), "at-most has no floor");
    assert_eq!(props["key"]["minLength"], 26, "the exact form pins both");
    assert_eq!(props["key"]["maxLength"], 26);

    for (label, code) in [
        ("rust-sdk", RustSdkGenerator::generate(&schema).unwrap().code),
        ("python-sdk", PythonSdkGenerator::generate(&schema).unwrap().code),
        ("go-sdk", GoSdkGenerator::generate(&schema).unwrap().code),
        ("go", GoGenerator::generate(&schema).unwrap().code),
    ] {
        assert!(
            !code.contains("StringN") && !code.contains("string_n"),
            "{label} leaked the storage spelling onto the wire"
        );
    }
}

// ---------------------------------------------------------------------------
// #266: a foreign key's type and column width follow the TARGET model's
// identity type.  Before this, `map_field_type_ident` typed every FK scalar
// `Uuid` and `RustGenerator::is_uuid_pk` skipped every non-UUID-keyed target at
// 21 sites — silently dropping referential integrity, `@on_delete`, forward
// traversal, reverse getters, eager load, and M2M junctions.  It compiled, ran,
// and warned about nothing.
//
// The invariant these scenarios pin:
//
//   An FK column is physically identical to the column the target's identity
//   field itself occupies.
//
// Fixtures are inline: all 18 corpus schemas are UUID-keyed and are the
// zero-churn control (scenario 1) — they must stay untouched.
// ---------------------------------------------------------------------------

/// A `u64`-keyed parent with a one-to-many back-reference, a required FK child,
/// an optional FK child field, and a self-FK.  The workhorse fixture for the
/// widening scenarios.
const U64_PARENT_SRC: &str = r#"
Post {
  id: +u64
  title: string
  comments: [Comment]
}

Comment {
  id: +uuid
  body: string
  post: *Post
  reply_to: ?Comment
}
"#;

/// **Scenario 1 (the control).** For a UUID-keyed target the resolution returns
/// `FieldType::Uuid`, and every helper's `Uuid` arm is byte-for-byte what its FK
/// arm hardcoded — so nothing about a conventional schema moves.  The operative
/// proof is procedural (the whole snapshot suite accepts nothing); this pins the
/// four physical facts that would have to change first if it were not true.
///
/// If this is red, resolution 5's "not a format break" claim is void.
#[test]
fn test_rust_generation_fk_to_a_uuid_key_is_byte_identical() {
    let src = r#"
Post {
  id: +uuid
  title: string
  comments: [Comment]
}

Comment {
  id: +uuid
  post: *Post
  editor: ?Post
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub post: Uuid"), "FK record field stays `Uuid`");
    assert!(
        code.contains("comment/fixed/uuid_1.bin"),
        "FK column keeps the `uuid` file-path label — a rename reads as an empty database"
    );
    assert!(code.contains("16usize"), "FK column keeps its 16-byte width");
    assert!(
        code.contains("forgedb_storage::ColumnType::Uuid"),
        "FK manifest entry keeps `ColumnType::Uuid`"
    );
    assert!(code.contains("append_uuid"), "FK write path keeps `append_uuid`");
    assert!(
        code.contains("size_of::<Option<Uuid>>()"),
        "the optional FK keeps `Option<Uuid>` sizing"
    );
}

/// **Scenario 2.** The width, the Rust type, the file-path label and the
/// manifest `ColumnType` all follow the target's identity.  Today the column is
/// 16 bytes of `Uuid` that nothing can ever populate with a matching value.
#[test]
fn test_rust_generation_fk_follows_a_u64_key() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("pub post: u64"),
        "the FK scalar is typed as the target's key, not `Uuid`"
    );
    assert!(
        code.contains("comment/fixed/u64_"),
        "the FK column file is labelled by the resolved type — a wrong label \
         renames the column file, which reads as a fresh empty database"
    );
    // `Comment.id` is `+uuid`, so a `uuid` column legitimately exists on the
    // model — what must be gone is the FK's own one.  `post` is field index 2.
    assert!(
        !code.contains("comment/fixed/uuid_2.bin"),
        "the FK no longer occupies a `uuid`-labelled column"
    );
    assert!(
        code.contains("forgedb_storage::ColumnType::U64"),
        "the manifest entry is the target key's ColumnType"
    );
    assert!(code.contains("append_u64"), "the FK write path uses the u64 accessor");
    assert!(
        !code.contains("record.post.as_bytes()"),
        "the FK is no longer written through the uuid byte path"
    );
}

/// **Scenario 3.** Every relation capability `is_uuid_pk` was silently skipping
/// is emitted.  Asserted as *presence*: absence was the bug, and absence is what
/// a regression looks like.
#[test]
fn test_rust_generation_u64_key_gets_the_whole_relation_surface() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("pub fn comment_post(&self, record: &Comment) -> Option<Post>"),
        "forward traversal getter"
    );
    assert!(
        code.contains("pub fn post_comments(&self, id: u64) -> Vec<Comment>"),
        "reverse collection getter, keyed by the parent's own id type"
    );
    assert!(
        code.contains("pub struct CommentWithRelations"),
        "eager-load struct"
    );
    assert!(
        code.contains("pub fn comment_with_relations(&self, id: Uuid) -> Option<CommentWithRelations>"),
        "eager-load getter"
    );
    assert!(
        code.contains("self.comment.find_by_post(id)"),
        "the reverse getter probes the child's FK index rather than scanning (#100)"
    );
}

/// **Scenario 4.** Deleting a referenced `Post` is refused.  Today it succeeds
/// and orphans every child — the delete wrapper is a bare delegate for a
/// non-UUID key.
#[test]
fn test_rust_generation_non_uuid_parent_delete_is_referentially_checked() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("pub fn delete_post(&mut self, id: u64)"),
        "the delete wrapper takes the parent's own key type"
    );
    assert!(
        code.contains("pub fn delete_post_cascade") || code.contains("fn delete_post_cascade"),
        "the depth-bounded cascade worker is emitted for a u64-keyed parent"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("ValidationError::ReferencedByChildren{model:\"Comment\",field:\"post\",}"),
        "default `restrict` refuses the delete (409) instead of orphaning the child"
    );
}

/// **Scenario 5.** `@on_delete(cascade)` and `@on_delete(set_null)` both fire
/// against a `u64`-keyed parent.
#[test]
fn test_rust_generation_on_delete_policies_fire_for_a_u64_parent() {
    let src = r#"
Post {
  id: +u64
  title: string
  comments: [Comment]
  drafts: [Draft]
}

Comment {
  id: +uuid
  post: *Post @on_delete(cascade)
}

Draft {
  id: +uuid
  post: ?Post @on_delete(set_null)
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        flat.contains("self.delete_comment_cascade(__c.id,__depth+1)?"),
        "cascade recurses into the referencing child"
    );
    assert!(
        flat.contains("__c.post=None;"),
        "set_null nulls the optional FK on the referencing child"
    );
    assert!(
        flat.contains("self.draft.find_by_post(Some(id))"),
        "the set_null child probe passes the parent key wrapped in Some"
    );
}

/// **Scenario 6.** The generated prose asserted the false premise for the whole
/// life of the bug.  A doc comment that lies is what made this invisible
/// (`silent-capability-holes-in-codegen`), so its absence is a scenario.
#[test]
fn test_rust_generation_delete_doc_does_not_deny_a_referencing_model() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        !code.contains("no model references it via a foreign key"),
        "the delete wrapper claimed nothing references a u64-keyed model, in a \
         schema where `Comment.post` does"
    );
    assert!(
        !code.contains("integer-keyed"),
        "an integer key is no longer a second-class FK target, so nothing calls \
         it out as one"
    );
}

/// **Scenario 7 (resolution 1).** `Order { id: *Customer }` — an identity that is
/// itself a foreign key.  Everything about the emitted key is already `Uuid`, yet
/// this shape passed `is_uuid_pk` nowhere, so it lost the relation surface for a
/// reason that was never true.
#[test]
fn test_rust_generation_identity_that_is_itself_an_fk_resolves_through() {
    let src = r#"
Customer {
  id: +uuid
  name: string
  orders: [Order]
}

Order {
  id: *Customer
  total: i64
  lines: [Line]
}

Line {
  id: +uuid
  order: *Order
  qty: u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub id: Uuid"), "the FK identity resolves to Uuid");
    assert!(
        code.contains("pub order: Uuid"),
        "an FK pointing AT the FK-keyed model resolves through the chain"
    );
    assert!(
        code.contains("pub fn line_order(&self, record: &Line) -> Option<Order>"),
        "the chain-keyed model is an ordinary FK target"
    );
    assert!(
        code.contains("pub fn customer_orders(&self, id: Uuid) -> Vec<Order>"),
        "and an ordinary FK source"
    );
}

/// **Scenario 9.** The OpenAPI document described every FK as
/// `{"type":"string","format":"uuid"}`.  For a `u64`-keyed target that is not
/// merely unhelpful, it is wrong: the server serializes a JSON number.
#[test]
fn test_api_generation_openapi_fk_follows_the_target_key() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let doc = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&doc).unwrap();

    let post = &spec["components"]["schemas"]["Comment"]["properties"]["post"];
    assert_eq!(post["type"], "integer", "an FK to a u64 key is an integer: {post}");
    assert_eq!(post["format"], "int64", "{post}");

    // The control: an FK to a uuid-keyed target is unchanged.
    let reply = &spec["components"]["schemas"]["Comment"]["properties"]["reply_to"];
    assert_eq!(reply["format"], "uuid", "a uuid-keyed FK target is untouched: {reply}");
}

/// **Scenario 10.** The three REST SDKs deserialize the FK; a `u64` arriving as a
/// JSON number into a `String`/`str`/`string` field is a client that cannot parse
/// its own server's response.
#[test]
fn test_sdk_generation_fk_follows_the_target_key() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();

    let rust = RustSdkGenerator::generate(&schema).unwrap().code;
    assert!(
        rust.contains("pub post: u64"),
        "rust-sdk types the FK as the target key:\n{rust}"
    );

    let python = PythonSdkGenerator::generate(&schema).unwrap().code;
    assert!(
        python.contains("post: int"),
        "python-sdk types the FK as the target key:\n{python}"
    );

    let go = GoSdkGenerator::generate(&schema).unwrap().code;
    assert!(
        go.contains("Post uint64"),
        "go-sdk types the FK as the target key:\n{go}"
    );
}

/// **Scenario 11.** The three in-process bindings compile either way — they
/// `.to_string()` the FK — so the defect is silent: the FK surfaces as `"42"`
/// while the target's own `id` surfaces as `42` on the very same object.
///
/// Asserted as an EQUALITY between the two mappings rather than against a
/// literal, so it cannot rot when a key type is added.
#[test]
fn test_bindings_fk_type_equals_the_targets_own_id_type() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();

    // Go binding: the struct field for `Comment.post` must be spelled the same
    // as `Post.id`'s own field.
    let go = GoGenerator::generate(&schema).unwrap().code;
    let field_type = |decl: &str, name: &str| {
        let body = &go[go.find(decl).unwrap_or_else(|| panic!("`{decl}` in the Go binding"))..];
        body.lines()
            .take_while(|l| !l.starts_with('}'))
            .find(|l| l.trim_start().starts_with(&format!("{name} ")))
            .map(|l| l.split_whitespace().nth(1).unwrap().to_string())
            .unwrap_or_else(|| panic!("field `{name}` in `{decl}`"))
    };
    let post_id = field_type("type Post struct {", "Id");
    let comment_fk = field_type("type Comment struct {", "Post");
    assert_eq!(
        comment_fk, post_id,
        "the Go FK field and the target's own id field must have one type"
    );

    // NAPI + PyO3: the FK must not be stringified when the key is not a uuid.
    let napi = NapiGenerator::generate(&schema).unwrap().code;
    assert!(
        !napi.contains("record.post.to_string()"),
        "napi stringifies a u64 FK while `Post.id` surfaces as a number"
    );
    let pyo3 = PyO3Generator::generate(&schema).unwrap().code;
    assert!(
        !pyo3.contains("record.post.to_string()"),
        "pyo3 stringifies a u64 FK while `Post.id` surfaces as a number"
    );
}

/// A mixed-key many-to-many: `Student` is `u64`-keyed, `Course` is `uuid`-keyed.
/// `valid_m2m` excludes this pair entirely today.
///
/// `detect_many_to_many_relations` names the pair `(Course, Student)` — the
/// junction's LEFT endpoint is the uuid-keyed `Course` and its RIGHT is the
/// u64-keyed `Student`, which is why the assertions below read 16 then 8.
const MIXED_M2M_SRC: &str = r#"
Student {
  id: +u64
  name: string
  courses: [Course]
}

Course {
  id: +uuid
  title: string
  students: [Student]
}
"#;

/// **Scenario 12.** `valid_m2m` admits a mixed-key pair, and the junction's two
/// columns are the two endpoints' own widths — not a hardcoded 16/16.
#[test]
fn test_rust_generation_junction_admits_a_mixed_key_pair() {
    let mut parser = forgedb_parser::Parser::new(MIXED_M2M_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(code.contains("pub struct CourseStudentLink"), "the junction exists at all");
    assert!(
        flat.contains("root.join(\"course_student_link/fixed/left.bin\"),16usize,"),
        "the left column is the uuid endpoint's width"
    );
    assert!(
        flat.contains("root.join(\"course_student_link/fixed/right.bin\"),8usize,"),
        "the right column is the u64 endpoint's width"
    );
    assert!(
        flat.contains("pubfnlink_course_student(&mutself,left:Uuid,right:u64)"),
        "the link helper takes each endpoint's own key type"
    );
    assert!(
        flat.contains("left_index:std::collections::HashMap<Uuid,Vec<u64>>")
            && flat.contains("right_index:std::collections::HashMap<u64,Vec<Uuid>>"),
        "the traversal indexes key on the endpoint types (#154)"
    );
}

/// **Scenario 13.** `link` / `unlink` / `pairs` and the rehydration pass all run
/// over the endpoint key types — the five places the junction hardcoded the UUID
/// pair, none of which greps for `is_uuid_pk`.
#[test]
fn test_rust_generation_junction_round_trip_is_key_typed() {
    let mut parser = forgedb_parser::Parser::new(MIXED_M2M_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        flat.contains("self.left_col.append_uuid(*left.as_bytes())"),
        "link appends the left endpoint through its own accessor"
    );
    assert!(
        flat.contains("self.right_col.append_u64(right)"),
        "...and the right one through its own"
    );
    assert!(
        flat.contains("pubfnpairs(&self)->Vec<(Uuid,u64)>"),
        "pairs() yields the endpoint key types"
    );
    assert!(
        flat.contains("self.right_col.read_u64(i)"),
        "the rehydration/latest-wins pass reads the right endpoint as u64"
    );
    assert!(
        flat.contains("pubfnrights_of(&self,left:Uuid)->Vec<u64>")
            && flat.contains("pubfnlefts_of(&self,right:u64)->Vec<Uuid>"),
        "the traversal probes are key-typed"
    );
    assert!(
        flat.contains("pubfncourse_students(&self,id:Uuid)->Vec<Student>")
            && flat.contains("pubfnstudent_courses(&self,id:u64)->Vec<Course>"),
        "the Database-level M2M getters take each side's own key"
    );
}

/// **Scenario 14.** The replication follower frames a junction link as an opaque
/// byte pair.  Its width is `left + right`, and the literal `32` is the single
/// thing most likely to survive a careless edit.
#[test]
fn test_rust_generation_junction_replay_frame_is_the_endpoint_widths() {
    let mut parser = forgedb_parser::Parser::new(MIXED_M2M_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        flat.contains("ev.bytes.len()==24"),
        "the replay arm accepts a 16 + 8 frame"
    );
    assert!(
        !flat.contains("ev.bytes.len()==32"),
        "...and no longer a 32-byte one for this junction"
    );
    assert!(
        flat.contains("Vec::with_capacity(24)"),
        "the broker record reserves the same width it writes"
    );
    assert!(
        flat.contains("right.to_le_bytes()"),
        "an integer endpoint is framed little-endian, matching its column"
    );
    assert!(
        flat.contains("<u64>::from_le_bytes((&ev.bytes[16..24])"),
        "...and the follower decodes that slot back at the right offset"
    );
}

// ---------------------------------------------------------------------------
// #254 — a timestamp identity is a real key: FKs follow it, junctions hold it
// ---------------------------------------------------------------------------

/// **#254 x #266.** An FK whose target is keyed on `+timestamp(us)` resolves to
/// `Timestamp` — the type/width/label/ColumnType chain #266 built has to carry a
/// key type that no identity could have before this issue.
///
/// The failure this pins is not a compile error: `map_field_type_ident` would
/// fall back to `Uuid`, and the FK column would be 16 bytes that nothing can
/// ever populate with a matching value.
#[test]
fn test_rust_generation_fk_follows_a_timestamp_key() {
    let src = r#"
Tick {
  id: +timestamp(us)
  label: string
  samples: [Sample]
}

Sample {
  id: +u64
  tick: *Tick @on_delete(cascade)
  reading: f64
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("pub tick: Timestamp"),
        "the FK scalar follows the target's timestamp identity, not `Uuid`"
    );
    assert!(
        code.contains("sample/fixed/timestamp_1.bin"),
        "the FK column file is labelled by the resolved type"
    );
    assert!(
        !code.contains("sample/fixed/uuid_1.bin"),
        "the FK no longer occupies a `uuid`-labelled column"
    );
    assert!(
        code.contains("forgedb_storage::ColumnType::Timestamp"),
        "the manifest entry is the target key's ColumnType"
    );
    assert!(
        code.contains("append_timestamp(i64::from(record.tick))"),
        "the FK write path uses the timestamp accessor"
    );
    // The relation surface #266 unblocked is present for this key too — absence
    // was the bug class, so presence is what a regression would remove.
    assert!(
        code.contains("pub fn sample_tick(&self, record: &Sample) -> Option<Tick>"),
        "forward traversal over a timestamp FK"
    );
    assert!(
        code.contains("pub fn tick_samples(&self, id: Timestamp) -> Vec<Sample>"),
        "reverse getter keyed on the timestamp identity"
    );
    assert!(
        code.contains("pub fn find_by_tick(&self, value: Timestamp)"),
        "the FK lookup index is keyed on the resolved type"
    );
    assert!(
        code.contains("pub fn delete_tick(&mut self, id: Timestamp)"),
        "@on_delete(cascade) is wired over a timestamp-keyed parent"
    );
}

/// **#254 x #266.** A many-to-many junction stores each endpoint's id in a
/// fixed-width, hashable column — `FieldType::is_junction_key` already admits
/// `FieldType::Timestamp(_)`, and the `(_)` absorbs the precision parameter this
/// issue added, so #254 widens nothing here. This proves it by generation rather
/// than by reading the predicate: the junction exists, and it is keyed on
/// `Timestamp` on the timestamp side.
#[test]
fn test_rust_generation_timestamp_key_is_a_junction_endpoint() {
    let src = r#"
Tick {
  id: +timestamp(us)
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  ticks: [Tick]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    // The parser's own junction floor must not reject it either — if the
    // validator and `junction_key_type` ever disagree, the relation silently
    // vanishes again, which is exactly the failure #266 exists to remove.
    assert!(
        forgedb_parser::validate_schema(&schema).is_empty(),
        "a timestamp-keyed model is a legal junction endpoint"
    );
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub struct TagTickLink"), "the junction is generated");
    assert!(
        code.contains("pub fn link_tag_tick(&mut self, left: Uuid, right: Timestamp)"),
        "the junction links a uuid endpoint to a timestamp endpoint"
    );
    assert!(
        code.contains("left_index: std::collections::HashMap<Uuid, Vec<Timestamp>>")
            && code.contains("right_index: std::collections::HashMap<Timestamp, Vec<Uuid>>"),
        "the traversal indexes are keyed on each endpoint's own key type"
    );
    assert!(
        code.contains(".append_timestamp(i64::from(right))"),
        "the junction's timestamp column uses the timestamp accessor"
    );
}

// ---------------------------------------------------------------------------
// #254 — the engine-format migration hop, and the two-arm open guard
// ---------------------------------------------------------------------------

/// A schema whose timestamps sit in every position a schema-blind column pass
/// cannot see: bare, nullable, inside a fixed array, and inside a struct.
fn engine_hop_schema() -> Schema {
    let src = r#"
struct Window {
  opened_at: timestamp(s)
  closed_at: timestamp(us)
}

Reading {
  id: +uuid
  taken_at: timestamp(ms)
  maybe_at: ?timestamp(s)
  marks: [timestamp(s); 3]
  window: Window
  label: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    parser.parse().unwrap()
}

/// The engine hop reaches EVERY timestamp leaf, not only the bare ones.
///
/// This is the guard for the finding the plan's `verify` pass turned up: only a
/// bare `FieldType::Timestamp` becomes `ColumnType::Timestamp`, so a schema-blind
/// column pass would silently skip the nullable / arrayed / struct-nested ones —
/// 81 of 247 timestamp fields in the example corpus are nullable. A migration
/// that skips a third of the data compiles and runs perfectly.
#[test]
fn test_engine_generation_reaches_every_timestamp_leaf() {
    let schema = engine_hop_schema();
    let plan = EngineHopPlan {
        schema: &schema,
        schema_version: 1,
        from_engine: 1,
        to_engine: 2,
    };
    let code = EngineMigrationGenerator::generate_main_code(&plan).unwrap().code;

    // The bare field.
    assert!(
        code.contains(r#"__rescale(__row.taken_at, "Reading", "taken_at")"#),
        "a bare timestamp is rescaled: {code}"
    );
    // The nullable field — reached through the `Option`, not skipped.
    assert!(
        code.contains("if let Some(__ts_opt) = &mut __row.maybe_at"),
        "a NULLABLE timestamp is reached (the shape the schema-blind pass misses): {code}"
    );
    // The array — element-wise.
    assert!(
        code.contains("for __ts_elem in __row.marks.iter_mut()"),
        "every array element is rescaled: {code}"
    );
    // The struct — by field path, with the dotted name in the diagnostic.
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains(r#""Reading","window.opened_at""#)
            && flat.contains(r#""Reading","window.closed_at""#),
        "struct-nested timestamps are rescaled and named by path: {code}"
    );
    // The non-timestamp field is left alone.
    assert!(
        !code.contains("__row.label ="),
        "a string field is not touched: {code}"
    );
    // Overflow is detected, not wrapped: a stored second count past ~year 9999
    // would otherwise become a nonsensical instant, silently.
    assert!(
        code.contains("checked_mul(1000000)"),
        "the multiply is checked: {code}"
    );
    // Two modules of the SAME schema — the version interlock comes for free.
    assert!(
        code.contains("mod e1;") && code.contains("mod e2;"),
        "both engine generations are embedded: {code}"
    );
    assert!(
        code.contains("e1::Database::open_at") && code.contains("e2::Database::open_at"),
        "the reader half is e1 and the writer half is e2: {code}"
    );
}

/// The two embedded modules differ ONLY in the baked engine generation. If they
/// differed in the schema serial too, each module's own open-guard would refuse
/// the dir the other half just wrote.
#[test]
fn test_engine_generation_bakes_one_schema_two_generations() {
    let schema = engine_hop_schema();
    let plan = EngineHopPlan {
        schema: &schema,
        schema_version: 7,
        from_engine: 1,
        to_engine: 2,
    };
    let out = EngineMigrationGenerator::generate(&plan, "forgedb-engine-migrate").unwrap();
    let e1 = &out.sources.iter().find(|(p, _)| p == "src/e1.rs").expect("e1").1;
    let e2 = &out.sources.iter().find(|(p, _)| p == "src/e2.rs").expect("e2").1;

    assert!(e1.contains("EXPECTED_SCHEMA_VERSION: u32 = 7"));
    assert!(e2.contains("EXPECTED_SCHEMA_VERSION: u32 = 7"));
    assert!(e1.contains("EXPECTED_ENGINE_VERSION: u32 = 1"));
    assert!(e2.contains("EXPECTED_ENGINE_VERSION: u32 = 2"));
}

/// A generation pair the generator does not know is REFUSED, not silently given
/// the seconds→micros multiply. A wrong rescale corrupts exactly the data the
/// migration claims to carry, and it corrupts it irreversibly.
#[test]
fn test_engine_generation_refuses_an_unknown_hop() {
    let schema = engine_hop_schema();
    for (from, to) in [(2u32, 3u32), (1, 3), (2, 1)] {
        let plan = EngineHopPlan {
            schema: &schema,
            schema_version: 1,
            from_engine: from,
            to_engine: to,
        };
        let err = match EngineMigrationGenerator::generate(&plan, "x") {
            Err(e) => e,
            Ok(_) => panic!("only 1 -> 2 exists, but {from} -> {to} generated"),
        };
        assert!(
            err.to_string().contains("the only hop is 1 → 2"),
            "and it says which hop DOES exist: {err}"
        );
    }
}

/// The open guard's two arms describe two different situations with two
/// different remedies, and must never collapse into one message: sending a user
/// to the app's migration bin when ForgeDB's format changed would tell them to
/// regenerate a schema that is already correct.
#[test]
fn test_rust_generation_open_guard_has_two_distinct_arms() {
    let schema = engine_hop_schema();
    let code = RustGenerator::generate(&schema).unwrap().code;
    // prettyplease wraps the panic strings across source lines, so match the
    // flattened form — dropping the line-continuation backslashes too, which is
    // what makes a wrapped message one contiguous span again.
    let flat: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\\')
        .collect();

    assert!(
        flat.contains("isatschemaversionv"),
        "the schema arm names the app's serial: {code}"
    );
    assert!(
        flat.contains("waswrittenbyengineformatgeneration"),
        "the engine arm names ForgeDB's generation: {code}"
    );
    assert!(
        flat.contains("forgedbmigrateengine--src<dir>--dest<new-dir>"),
        "and points at the engine command, NOT the app's transformer: {code}"
    );
    assert!(
        flat.contains("yourschemadidnot"),
        "and says explicitly which of the two things changed: {code}"
    );
    assert!(
        flat.contains("engine_version:EXPECTED_ENGINE_VERSION"),
        "a written manifest stamps the generation this binary speaks: {code}"
    );
}

// ===========================================================================
// #252 — `string(N)` / `string(N!)` identities, backed by a `Copy` `InlineStr`.
//
// Every failure mode here is a *silent* one, which is why each scenario asserts
// a literal rather than "it generated". `map_field_type_ident` falls through to
// `String`, `api.rs::id_parse_type` falls through to `Uuid`, and
// `wasm.rs::pk_parse_opt` falls through to `Uuid::parse_str` — a missing arm in
// any of the three produces a handler that parses the wrong type rather than a
// build error. That is #265's `is_uuid_pk` shape, three more times.
// ===========================================================================

/// `api.rs` for a schema source.
fn api_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    ApiGenerator::generate(&schema).unwrap().code
}

/// The wasm read-replica's `replica.rs` for a schema source.
fn wasm_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    WasmGenerator::generate(&schema).unwrap().code
}

fn flat(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// **Scenario 16.** The key type is `InlineStr<N>`, with N in **bytes** and the
/// mapping the identity function.
///
/// The explicit guard against the withdrawn `4N` sizing: the previous Gate 1 was
/// written against #238's inline-or-overflow design and sized `InlineStr<104>`
/// for a 26-character key. #261 measured that design losing, #238 withdrew it,
/// and one byte per character plus res 3's `@utf8` ban make the width exactly N.
/// A `4N` key would still *compile*, which is why this asserts the literal.
#[test]
fn test_rust_generation_string_identity_is_inline_str_at_n_bytes() {
    let code = db_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("InlineStr<26usize>"),
        "a 26-character key is `InlineStr<26>`: {f:.400}"
    );
    assert!(
        !f.contains("InlineStr<104usize>"),
        "NOT the withdrawn 4N sizing"
    );
    // Both spellings map to the same width — `string(N!)`'s dead `len` is the
    // price of one key type rather than two (res 2).
    let atmost = flat(&db_for("Doc {\n  id: string(26)\n  title: string\n}\n"));
    assert!(
        atmost.contains("InlineStr<26usize>"),
        "`string(26)` keys the same width as `string(26!)`"
    );
}

/// A **non**-identity `string(N)` stays a plain `String`.
///
/// The corollary of res 7 and #238's decision 5: `InlineStr` exists because a key
/// is passed by value, and a column that is never a key has no such requirement.
/// Widening the mapping to every inline string would change the public struct
/// type of a feature that shipped in this same cycle, and — decisively — could
/// not be expressed: `map_field_type_ident` sees a `FieldType`, not a `Field`, so
/// it cannot see `@utf8` and cannot compute the `4N` width a non-identity column
/// needs.
#[test]
fn test_rust_generation_a_non_key_inline_string_is_still_a_string() {
    let code = db_for("Doc {\n  id: +uuid\n  sku: string(26!)\n  tag: string(8) @utf8\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("pub sku: String"),
        "a non-identity inline string is a `String`: {f:.400}"
    );
    assert!(
        !f.contains("InlineStr"),
        "and no key type appears anywhere in a uuid-keyed schema"
    );
}

/// **Scenario 19.** The in-memory identity maps are keyed on the key type, and
/// the create handler renders the key as text.
///
/// `id_to_row` / `id_versions` are `HashMap`s, which is why `InlineStr` hashes on
/// its text (#252 res 8): a key type whose `Eq` and `Hash` disagreed would
/// produce lookup misses rather than a build error.
#[test]
fn test_rust_generation_string_key_reaches_the_identity_maps() {
    let code = db_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("id_to_row: std::sync::Arc<HashMap<InlineStr<26usize>, usize>>"),
        "the row index is keyed on the key type: {f:.600}"
    );
    assert!(
        f.contains("id_versions: std::sync::Arc<HashMap<InlineStr<26usize>, Vec<usize>>>"),
        "and so is the version index"
    );
    assert!(
        f.contains("pub id: InlineStr<26usize>"),
        "and the model struct's own field agrees with them"
    );
}

/// **Scenario 20.** Wherever the key type lands in a `utoipa::ToSchema` context
/// it is annotated as a `String`.
///
/// `InlineStr` does not implement `ToSchema` — the same situation `Timestamp` and
/// `Decimal` are already in — so without the annotation the generated crate does
/// not compile at all. The live-query delta enum is the site most easily missed:
/// #254's compile check found it as a third, unrouted `Timestamp` site after the
/// model struct and the projection structs were both already handled.
#[test]
fn test_rust_generation_string_key_is_annotated_for_utoipa() {
    let code = db_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("#[schema(value_type = String)] pub id: InlineStr<26usize>"),
        "the model struct's key is documented as the string it serializes to: {f:.600}"
    );
    let at = f.find("Removed {").expect("the live-delta enum has a Removed variant");
    let window = &f[at.saturating_sub(120)..(at + 120).min(f.len())];
    assert!(
        window.contains("#[schema(value_type = String)]"),
        "and so is the delta enum's: {window}"
    );
}

/// **Scenario 21.** A `string(N)` identity's index key is the same `\u{1}` string
/// class a bare `string` of the same content produces.
///
/// The class byte is what keeps a string key from colliding with a numeric one in
/// the same index. Moving a key into a different class silently reorders every
/// range scan, and no test that reads back through the same index can tell.
#[test]
fn test_rust_generation_string_key_stays_in_the_string_index_class() {
    let code = db_for(
        "Doc {\n  id: string(26!)\n  code: &string(8!)\n  slug: &string\n}\n",
    );
    let f = flat(&code);
    assert!(
        f.contains(r#"write!(__k, "\u{1}{}", __v)"#) || f.contains(r#"\u{1}"#),
        "the inline key rides the string class: {f:.400}"
    );
    assert!(
        !f.contains(r#"write!(__k, "\u{2}{}", __v)"#) || f.contains(r#"\u{2}"#),
        "and not the numeric one"
    );
}

/// **Scenario 17.** The REST path parameter parses as the key type, not as a
/// `Uuid`.
///
/// `ApiGenerator::id_parse_type` has ended in `_ => quote! { Uuid }` since
/// 9a54319 (2026-07-06), so *every* non-integer, non-uuid identity has generated
/// an `api.rs` that does not compile — a bare `string` identity produces 32
/// errors of one class on an unmodified tree. #266 handed this forward
/// deliberately rather than write a test that could not fail; this is that test.
#[test]
fn test_api_generation_string_key_path_param_is_the_key_type() {
    // Gate 2 and #266's handoff both spell this `Sku { code: string(26) }`. That
    // model has no identity at all — #248 requires a field named `id` or a `+`
    // auto — so it cannot parse, let alone generate. Same class of unwriteable
    // fixture as #266's own scenario 17.
    let code = api_for("Sku {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    // The segment arrives as a `String` and is parsed in the handler (so a bad
    // key is a 400 with a message rather than axum's own rejection), which is
    // why `FromStr` on `InlineStr` is load-bearing and nothing is generated for
    // the extraction itself.
    assert!(
        f.contains("id.parse::<InlineStr<26usize>>()"),
        "the path segment parses into the key type: {f:.800}"
    );
    assert!(
        !f.contains("id.parse::<Uuid>()"),
        "and never falls through to Uuid for a string-keyed model"
    );
}

/// The fall-through itself is gone, not merely shadowed.
///
/// A `_ => Uuid` arm left in place would keep every future key type silently
/// wrong. This asserts the *shape*: a required-FK identity pointing at an
/// integer-keyed parent must parse as that integer, which the raw-`FieldType`
/// match could never do because it never resolved the relation.
#[test]
fn test_api_generation_id_parse_type_resolves_a_relation_identity() {
    let code = api_for(
        "Customer {\n  id: +u64\n  name: string\n}\n\nAccount {\n  id: *Customer\n  label: string\n}\n",
    );
    let f = flat(&code);
    assert!(
        f.contains("id.parse::<u64>()"),
        "an FK identity parses as the far end of the chain: {f:.800}"
    );
    assert!(
        !f.contains("id.parse::<Uuid>()"),
        "the old raw-FieldType match sent BOTH models' segments to Uuid: {f:.800}"
    );
}

/// **Scenario 18.** The browser read-replica parses a string key as itself.
///
/// The arm most likely to be forgotten: the replica is not in the default build,
/// so nothing else in the suite exercises it. `pk_parse_opt` falls through to
/// `Uuid::parse_str(&id).ok()`, which returns `None` for every well-formed string
/// key — a replica that silently resolves nothing.
#[test]
fn test_wasm_generation_replica_parses_a_string_key() {
    let code = wasm_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("<InlineStr<26usize>>::try_from(id.as_str()).ok()"),
        "the replica parses the key as the key type: {f:.800}"
    );
    assert!(
        !f.contains("Uuid::parse_str(&id).ok()"),
        "and never as a uuid"
    );
}

/// **Scenarios 12 / 14 / 15.** The generated write path carries the identity's
/// three value rules, and the alphabet check names the offending character and
/// its position.
///
/// Res 4 (URL-path-safe), res 5 (non-empty) and #238's ASCII restriction — which
/// for a *key* is unconditional, because res 3 removes the `@utf8` escape. All
/// three are generated rather than substrate (res 7): they apply because the
/// field is an identity, which `InlineStr` cannot know.
#[test]
fn test_rust_generation_string_key_carries_its_value_rules() {
    let code = db_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);

    assert!(
        f.contains(r#"rule: "identity_alphabet""#),
        "the path-segment alphabet is checked: {f:.600}"
    );
    assert!(
        f.contains(r#"rule: "identity_empty""#),
        "and the empty key is refused"
    );
    // The diagnostic names the character AND its position — a key rejected with
    // "invalid character" and nothing else is unactionable when the value came
    // out of somebody else's system.
    let at = f
        .find(r#"rule: "identity_alphabet""#)
        .expect("the alphabet rule is emitted");
    let window = &f[at..(at + 400).min(f.len())];
    assert!(
        window.contains("__c") && window.contains("__i"),
        "the message names the character and its byte offset: {window}"
    );
}

/// **Scenario 13.** The alphabet is `pchar` minus `%`, not the tighter
/// *unreserved* set.
///
/// The guard against silently tightening: `@` and `:` are sub-delims/`pchar` and
/// must be admitted, because `user@example.com` as a natural key is exactly the
/// ingestion scenario this issue exists for — and because #254's `+timestamp(us)`
/// key already renders `:` into its own path segment, so tightening here would
/// give the two identity types two different path rules.
#[test]
fn test_rust_generation_string_key_alphabet_is_pchar_minus_percent() {
    let code = db_for("Doc {\n  id: string(64)\n  title: string\n}\n");
    let f = flat(&code);
    let at = f
        .find("__forgedb_identity_char_ok")
        .expect("the alphabet predicate is emitted as a named helper");
    let window = &f[at..(at + 900).min(f.len())];
    // `'` is emitted escaped (`'\''`), so it is checked by its escaped spelling.
    for admitted in ["'@'", "':'", "'!'", "'$'", "'&'", r"'\''", "'('", "')'", "'*'", "'+'",
                     "','", "';'", "'='", "'-'", "'.'", "'_'", "'~'"] {
        assert!(
            window.contains(admitted),
            "{admitted} is a pchar and must be admitted: {window}"
        );
    }
    // ...and the alphanumerics come from the predicate rather than a list.
    assert!(
        window.contains("is_ascii_alphanumeric"),
        "letters and digits are admitted by predicate: {window}"
    );
    for rejected in ["'%'", "'/'", "'?'", "'#'", "'['", "']'"] {
        assert!(
            !window.contains(rejected),
            "{rejected} must NOT be admitted — `%` in particular, so the segment is \
             byte-identical to the key: {window}"
        );
    }
}

/// **Scenario 11.** A `string(N)`-keyed model is an ordinary foreign-key target:
/// the full relation surface generates, at the parent's key width.
///
/// Res 9, which is the resolution that constrained shipping. The failure this
/// guards is a *clean build with the relation surface silently absent* — which is
/// what #265 found and #266 fixed for integer keys. Asserting presence is the
/// point.
#[test]
fn test_rust_generation_string_keyed_target_keeps_the_relation_surface() {
    let code = db_for(
        "Airport {\n  id: string(3!)\n  city: string\n  flights: [Flight]\n}\n\n\
         Flight {\n  id: +uuid\n  origin: *Airport\n  alt: ?Airport\n}\n",
    );
    let f = flat(&code);

    // The FK column is the parent's key type, at the parent's width.
    assert!(
        f.contains("pub origin: InlineStr<3usize>"),
        "a required FK to a string-keyed parent is the parent's key: {f:.600}"
    );
    assert!(
        f.contains("pub alt: Option<InlineStr<3usize>>"),
        "and an optional FK is the same key, wrapped"
    );
    // ...and the column is physically identical to the parent's identity column.
    assert!(
        flat(&column_init(&code, "origin")).contains("3usize"),
        "the FK column is the identity column's width"
    );

    // Referential integrity, both delete modes, traversal and the reverse getter.
    assert!(f.contains("fn flight_origin"), "forward traversal exists");
    assert!(f.contains("fn flight_alt"), "and so does the optional one");
    assert!(
        f.contains("fn airport_flights_by_origin"),
        "the reverse collection getter exists"
    );
    assert!(
        f.contains("fn flight_with_relations"),
        "and the eager load"
    );
    assert!(
        f.contains("fn delete_airport"),
        "the parent's delete wrapper exists"
    );
    assert!(
        f.contains("if self.airport.get(record.origin).is_none()"),
        "referential integrity resolves the FK through the parent's own `get`, \
         which only type-checks if the FK and the key agree: {f:.600}"
    );
    assert!(
        f.contains("ValidationError::DanglingReference"),
        "and refuses a dangling reference"
    );
}

/// The M2M half of scenario 11 — #266's Gate 2 handoff 1.
///
/// #266 parameterized the junction on its two endpoint key types, but kept a
/// *physical floor* (`FieldType::is_junction_key`): fixed-width, hashable and
/// totally equatable, because the key sits in a `FixedColumn`, in a `HashMap`,
/// and in a fixed-width replication frame. `string(N)` satisfies all three — it
/// is a fixed slot and `InlineStr` is `Copy + Hash + Eq` — so the predicate
/// widens by one arm and the junction picks it up.
#[test]
fn test_rust_generation_string_key_is_a_junction_endpoint() {
    let code = db_for(
        "Isin {\n  id: string(12!)\n  name: string\n  funds: [Fund]\n}\n\n\
         Fund {\n  id: +uuid\n  label: string\n  holdings: [Isin]\n}\n",
    );
    let f = flat(&code);

    assert!(
        f.contains("fn link_fund_isin") || f.contains("fn link_isin_fund"),
        "the junction generates rather than silently vanishing: {f:.600}"
    );
    // The junction column is the endpoint's own key width, not a blanket 16.
    assert!(
        f.contains("FixedColumn::new") && f.contains("12usize"),
        "the string endpoint's junction column is 12 bytes wide"
    );
    // The traversal index is keyed on the key types, which is what needs
    // `Copy + Hash + Eq`.
    assert!(
        f.contains("HashMap<Uuid, Vec<InlineStr<12usize>>>")
            || f.contains("HashMap<InlineStr<12usize>, Vec<Uuid>>"),
        "the in-memory traversal index keys on the endpoint key types: {f:.600}"
    );
}

/// The validator and the generator must agree about which key types a junction
/// admits, because they cannot see each other: `FieldType::is_junction_key` on
/// the AST is the single predicate, and if the two sides drifted a relation would
/// silently vanish again — the exact failure #266 exists to remove.
#[test]
fn test_string_key_is_admitted_by_the_shared_junction_predicate() {
    assert!(
        FieldType::StringN {
            chars: 12,
            exact: true
        }
        .is_junction_key(),
        "the exact spelling is a junction key"
    );
    assert!(
        FieldType::StringN {
            chars: 32,
            exact: false
        }
        .is_junction_key(),
        "and so is the at-most spelling — both occupy a fixed slot"
    );
    assert!(
        !FieldType::String.is_junction_key(),
        "a bare `string` is variable-width and still cannot be one"
    );
}

/// The five breaks the compile-and-run check found that no snapshot could
/// (#252). Every one of them was a *green* snapshot suite over generated code
/// that did not compile — the exact reason CLAUDE.md makes compiling the output
/// a separate discipline from snapshotting it.
///
/// They share one root cause worth stating once: the inline-string branches were
/// gated on the DECLARED field type, and an FK's declared type is a relation. So
/// every one of them is a place where an FK to a string-keyed model had to be
/// recognized as the inline-string column it physically is.
#[test]
fn test_rust_generation_a_string_fk_is_an_inline_string_column() {
    let code = db_for(
        "Airport {\n  id: string(3!)\n  city: string\n  flights: [Flight]\n}\n\n\
         Flight {\n  id: +uuid\n  origin: *Airport\n  alt: ?Airport\n}\n",
    );
    let f = flat(&code);

    // 1. The column methods. `type_name` maps `StringN` to `inline_string` so
    //    the column FILE reads honestly (#238) — which means composing a method
    //    name from it names a method that does not exist. The FK must reach the
    //    pack/unpack path instead, exactly as a declared `string(N)` does.
    assert!(
        !f.contains("append_inline_string") && !f.contains("read_inline_string"),
        "no column method is named after the inline-string label: {f:.400}"
    );

    // 2. The transmute path is the one a nullable FK fell into, and it would
    //    have persisted the Rust `Option<InlineStr>` layout rather than the
    //    column's `[tag, payload]` framing.
    assert!(
        f.contains("let mut __buf = [0u8; 4usize]"),
        "a nullable string FK packs a tagged slot, not a transmuted Option: {f:.400}"
    );

    // 3. `#[schema(value_type = String)]` on the FK, not just on the identity —
    //    `InlineStr` implements no `ToSchema`, so the derive does not resolve.
    let flight = f
        .split("pub struct Flight {")
        .nth(1)
        .expect("the Flight struct")
        .split('}')
        .next()
        .expect("its body")
        .to_string();
    assert!(
        flight.contains("#[schema(value_type = String)] pub origin:"),
        "the required FK is annotated, not only the identity: {flight}"
    );
    assert!(
        flight.contains("#[schema(value_type = Option<String>)] pub alt:"),
        "and the optional one documents an optional string: {flight}"
    );

    // 4. The cascade/reverse-getter probe takes `&str` (an index probe over a
    //    string-semantic column does, deliberately), but these call sites hold
    //    the key by value.
    assert!(
        f.contains("self.flight.find_by_origin(&id)"),
        "the cascade borrows the key for the probe: {f:.400}"
    );
    assert!(
        f.contains("self.flight.find_by_alt(Some(&id))"),
        "and so does the optional one"
    );
}

/// A `string(N)` key is the one string-semantic field the scan view does NOT
/// borrow (#252). The scan *scope* (#228) returns a vector of ids that outlives
/// the buffers they were decoded from, so a borrowed key cannot escape it —
/// and `InlineStr<N>` being `Copy` means holding it by value costs the scan
/// nothing, since not allocating was the only thing the borrow ever bought.
#[test]
fn test_rust_generation_the_scan_view_holds_a_string_key_by_value() {
    let code = db_for("Airport {\n  id: string(3!)\n  city: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("pub struct AirportScanRef<'a> { pub id: InlineStr<3usize>"),
        "the scan view's key is owned and Copy: {f:.400}"
    );
    assert!(
        f.contains("pub city: &'a str"),
        "while an ordinary string column still borrows"
    );

    // ...and with the key no longer borrowing, a model whose ONLY string-
    // semantic field is its key borrows nothing at all, so #250's lifetime
    // anchor is needed again. Without it: `error[E0392]: lifetime parameter
    // 'a is never used`.
    let bare = flat(&db_for("Tag {\n  id: string(8!)\n  weight: u32\n}\n"));
    assert!(
        bare.contains("PhantomData<&'a ()>"),
        "a key-only string model re-anchors 'a: {bare:.400}"
    );
}

// ---- #251: the identity allow-list, one predicate, and the precedence fix ----

/// **Scenario 9 — shape 4, the silent mis-key, at the generator.**
///
/// ```forge
/// Event { seq: +u64  id: u32  note: string }
/// ```
///
/// Under the old first-match predicate this **compiles and runs**: the database
/// is keyed on `seq` while the generated parameter is *named* `id`, so
/// `/events/{id}` takes a sequence number and every relation pointing here points
/// at the wrong column. It is the only shape in #251 that produces a working
/// binary with the wrong key — which is why it is a v0.4.0 item rather than a
/// deferred cleanup, since changing a model's primary key later is an on-disk
/// format change for anyone who shipped it.
///
/// #254 fixed the four picking sites; #251 makes them one definition. The guard
/// belongs here, on this issue, because this is its defining defect.
#[test]
fn test_rust_generation_an_id_field_wins_over_an_auto_declared_above_it() {
    let code = db_for("Event {\n  seq: +u64\n  id: u32\n  note: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("pub fn get(&self, id: u32)"),
        "the key is the `id` field, not the `+u64` above it: {f:.400}"
    );
    assert!(
        !f.contains("pub fn get(&self, id: u64)"),
        "and emphatically not the sequence: {f:.400}"
    );
    assert!(
        f.contains("id_to_row: std::sync::Arc<HashMap<u32, usize>>"),
        "the identity map is keyed on `id`: {f:.400}"
    );

    // Declaration order must not matter — the same schema written the other way
    // round generates byte-identical code.
    let other = db_for("Event {\n  id: u32\n  seq: +u64\n  note: string\n}\n");
    assert_eq!(
        flat(&other).contains("pub fn get(&self, id: u32)"),
        true,
        "precedence, not position"
    );
}

/// The same precedence, in **every** generator that picks an identity. This is
/// the assertion the 37-site sweep exists for: before it, `validate.rs` could
/// select one field while `rust.rs` keyed on another, and the guard would then be
/// checking a field the database does not use.
#[test]
fn test_generators_agree_on_which_field_is_the_identity() {
    let src = "Event {\n  seq: +u64\n  id: u32\n  note: string\n}\n";

    let api = flat(&api_for(src));
    assert!(
        api.contains("id.parse::<u32>()"),
        "the REST path segment parses the `id` field's type: {api:.400}"
    );
    assert!(
        !api.contains("id.parse::<u64>()"),
        "not the sequence's: {api:.400}"
    );

    let wasm = flat(&wasm_for(src));
    assert!(
        wasm.contains("id.parse::<u32>()"),
        "the browser replica agrees: {wasm:.400}"
    );
    assert!(!wasm.contains("id.parse::<u64>()"), "{wasm:.400}");
}

/// A model whose identity is an auto under a **different** name keeps working —
/// `code: +uuid` with no `id` field is the #248 spelling the corpus uses, and
/// precedence must not quietly demote it. An allow-list that rejects too much is
/// as broken as one that rejects too little, and this failure is the quieter one.
#[test]
fn test_rust_generation_an_auto_under_another_name_is_still_the_identity() {
    let f = flat(&db_for("Token {\n  code: +uuid\n  label: string\n}\n"));
    assert!(
        f.contains("pub fn get(&self, id: Uuid)"),
        "the auto field serves as identity: {f:.400}"
    );
    assert!(
        f.contains("id_to_row: std::sync::Arc<HashMap<Uuid, usize>>"),
        "{f:.400}"
    );
}
