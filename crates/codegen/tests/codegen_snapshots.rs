use forgedb_source_guard::RustSource;
use forgedb_codegen::{
    ApiGenerator, FfiGenerator, GenConfig, GoGenerator, GoSdkGenerator, HopPlan, ModelOp,
    NapiGenerator, OpenApiGenerator, PyO3Generator, PythonSdkGenerator, RustGenerator,
    RustSdkGenerator, TransformGenerator, TransformPlan, TypeScriptGenerator, VersionSchema,
    WasmGenerator,
};
use forgedb_codegen::{EngineHopPlan, EngineMigrationGenerator};
use forgedb_parser::ast::{ComponentProtocol, ComponentReference, IndexType, RelationInclusion};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema, TimestampPrecision};

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

    assert!(code.contains("#[serde(default)]"), "uuid auto field needs #[serde(default)]");
    assert!(
        code.contains("#[serde(default = \"__forgedb_default_ts\")]"),
        "timestamp auto field needs the default-fn attr"
    );
    assert!(
        code.contains("fn __forgedb_default_ts() -> Timestamp"),
        "the timestamp default helper must be emitted"
    );

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

    assert!(result.code.contains("use utoipa::OpenApi"));

    assert!(result.code.contains("#[utoipa::path"));

    assert!(result.code.contains("#[derive(OpenApi)]"));
    assert!(result.code.contains("#[openapi"));

    assert!(result.code.contains("pub fn openapi_json"));
}

#[test]
fn test_api_generation_has_all_crud_operations() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    assert!(result.code.contains("async fn list_user"));
    assert!(result.code.contains("async fn get_user"));
    assert!(result.code.contains("async fn create_user"));

    assert!(result.code.contains("pub fn create_router"));
}

#[test]
fn test_api_generation_has_update_delete_endpoints() {
    let schema = simple_user_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("async fn update_user"));
    assert!(code.contains("async fn delete_user"));
    assert!(code.contains(".put(update_user)"));
    assert!(code.contains(".delete(delete_user)"));
    assert!(code.contains("db.update_user(key, record)"));
    assert!(code.contains("db.create_user(record)"));
    assert!(code.contains("db.delete_user(key)"));
}

#[test]
fn test_rust_generation_root_threading() {
    let schema = multi_model_schema();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub fn new_at(root: &std::path::Path)"));
    assert!(code.contains("pub fn new()"));
    assert!(code.contains("pub fn open_at(root: std::path::PathBuf)"));
    assert!(code.contains("root.join("));
    assert!(
        !code.contains("PathBuf::from(\""),
        "no hardcoded CWD-relative column paths should remain"
    );
    assert!(code.contains("fn write_manifest(&self, root: &std::path::Path)"));
}

#[test]
fn test_api_generation_tenant_auth_router() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub fn create_router("));
    assert!(code.contains("pub fn create_router_with_auth("));
    assert!(code.contains("auth: Arc<forgedb_auth::Authenticator>"));
    assert!(code.contains("forgedb_auth::axum_mw::require_tenant"));
    assert!(code.contains("axum::middleware::from_fn_with_state"));
}

#[test]
fn test_api_generation_cors_surface_is_additive() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("pub fn create_router("), "create_router keeps its arity");
    assert!(
        code.contains("pub fn create_router_with_auth("),
        "create_router_with_auth keeps its arity"
    );
    assert!(code.contains("pub fn create_router_with_options("));
    assert!(code.contains("pub fn create_router_with_auth_and_options("));
    assert!(code.contains("pub struct HttpOptions"));
    assert!(code.contains("pub fn parse_origins("));
    assert!(code.contains("pub struct AllowedOrigins"));
}

#[test]
fn test_api_generation_cors_allows_only_what_is_served() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    for m in ["Method::GET", "Method::POST", "Method::PUT", "Method::DELETE"] {
        assert!(code.contains(m), "CORS must allow {m}");
    }
    assert!(
        !code.contains("Method::PATCH"),
        "no PATCH route is generated, so CORS must not advertise it"
    );
    assert!(code.contains("header::CONTENT_TYPE"));
    assert!(code.contains("header::AUTHORIZATION"));
    assert!(
        !code.contains("allow_credentials("),
        "ForgeDB auth is a bearer header, not a cookie — credentials mode must stay \
         off, or the `*` origin becomes unsafe and tower-http rejects the combination"
    );
}

#[test]
fn test_api_generation_cors_layer_is_conditional() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("match cors"),
        "the CorsLayer must be applied only when origins are configured"
    );
    assert!(
        code.contains("layer(axum::Extension(AllowedOrigins("),
        "the AllowedOrigins extension must be applied unconditionally"
    );
    let trace = code.find("TraceLayer::new_for_http()").expect("trace layer emitted");
    let apply = code.find("__apply_origin_layers(router").expect("origin layers applied");
    assert!(
        trace < apply,
        "the origin layers must be applied AFTER TraceLayer so CORS ends up outermost"
    );
}

#[test]
fn test_api_generation_ws_handlers_check_origin() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    let extensions = code.matches("Extension<AllowedOrigins>").count();
    assert!(
        extensions >= 3,
        "all three WS upgrade handlers (/subscribe, /live-query, /replicate) must \
         take the allow-list; found {extensions}"
    );
    let checks = code.matches("allowed.permits(__origin_of(&headers))").count();
    assert!(
        checks >= 3,
        "all three WS upgrade handlers must gate on the origin; found {checks}"
    );
    assert!(
        code.contains("StatusCode::FORBIDDEN"),
        "a disallowed origin must be refused, not merely logged"
    );
}

#[test]
fn test_api_generation_observability_endpoints() {
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("async fn __health("));
    assert!(code.contains("async fn __ready("));
    assert!(code.contains("async fn __metrics("));
    assert!(code.contains("\"/health\""));
    assert!(code.contains("\"/ready\""));
    assert!(code.contains("\"/metrics\""));
    assert!(code.contains("TraceLayer::new_for_http()"));
    assert!(code.contains("fn __data_routes()"));
    assert!(code.contains("fn __ops_routes()"));
    assert!(code.contains(".row_count()"));
}

#[test]
fn test_api_generation_pagination_knobs() {
    let schema = multi_model_schema();

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
    assert!(
        c.contains("clamped to [1, 500]; default 25"),
        "the OpenAPI limit description reflects the baked bounds (#141)"
    );
}

#[test]
fn test_api_generation_metrics_toggle() {
    let schema = multi_model_schema();

    let d = ApiGenerator::generate(&schema).unwrap().code;
    assert!(d.contains("async fn __metrics("), "default emits __metrics (#151)");
    assert!(d.contains("\"/metrics\""), "default wires the /metrics route (#151)");

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
    let schema = multi_model_schema();
    let code = ApiGenerator::generate(&schema).unwrap().code;

    assert!(code.contains("all_at(&forgedb_storage::Snapshot::new(__w))"));
    assert!(code.contains("get_at(&forgedb_storage::Snapshot::new(__w)"));

    assert!(code.contains("params.get(\"as_of\")"));
    assert!(code.contains(".parse::<usize>()"));
    assert!(code.contains("StatusCode::BAD_REQUEST"));
    assert!(code.contains("as_of must be a non-negative integer watermark"));

    assert!(code.contains("match __as_of"));
    assert!(code.contains("_event_matches"));

    assert!(code.contains("async fn __snapshot("));
    assert!(code.contains("\"/snapshot\""));
    assert!(code.contains("\"watermarks\""));
    assert!(!code.contains("match model_name"));
}

#[test]
fn test_rust_generation_list_scan_narrow() {
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

    assert!(db_code.contains("pub struct UserScanRef<'a>"), "#160/#224: narrow scan view emitted");
    assert!(!db_code.contains("UserScanRow"),
        "#228: the owned scan record is gone — the scope is the only scan surface");
    assert!(!db_code.contains("fn __scan_row_at("),
        "#228: the per-row narrow decoder is gone with its only caller");
    assert!(!db_code.contains("to_owned_row"),
        "#228: nothing materializes a scan row");
    let scan_struct = &db_code[db_code.find("pub struct UserScanRef<'a>").unwrap()..];
    let scan_struct = &scan_struct[..scan_struct.find('}').unwrap()];
    assert!(scan_struct.contains("status") && scan_struct.contains("age"),
        "#160: scan view carries filterable fields");

    assert!(api_code.contains("fn __user_scan_matches("), "#160: narrow filter helper");
    assert!(api_code.contains("fn __user_scan_sort("), "#160: narrow sort helper");
    assert!(!api_code.contains("__user_scan_matches_ref"),
        "#228: one scan filter, not an owned/borrowed pair — the owned operand is gone");
    let api_flat: String = api_code.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        api_flat.contains("db .user .__with_page("),
        "#160/#224/#228/#226: live list scans narrowly, inside the page scope.\nGot: {api_flat}"
    );
    assert!(api_code.contains("__user_scan_matches(r, &params)"), "#160: live list filters narrow");
    assert!(!api_flat.contains(".filter_map(|__id| db.user.get(*__id))"),
        "#226: no second positional read of the page\nGot: {api_flat}");
    assert!(!api_flat.contains("db .user .__with_scan("),
        "#226: the owned narrow path is gone for a model with no @projection\nGot: {api_flat}");
    assert!(api_code.contains("all_at(&forgedb_storage::Snapshot::new(__w))"),
        "#160: as_of retains the full snapshot read");
    assert!(
        api_flat.contains(
            "|__scan: &mut Vec<super::UserScanRef<'_>>| { __user_scan_sort(__scan, &qp.sort); }"
        ),
        "#226: the scan callback sorts and nothing else.\nGot: {api_flat}"
    );
    assert!(
        api_flat.contains(
            "|__total: usize, __page: &[super::UserPageRef<'_>]| { ( StatusCode::OK, \
             Json(__ListEnvelope { data: __page, total: __total, limit: qp.pagination.limit, \
             offset: qp.pagination.offset, }), ) .into_response() }"
        ),
        "#226: the page serializes from the buffers; only an owned Response escapes.\nGot: {api_flat}"
    );
    assert!(
        api_flat.contains("return db .user .__with_page("),
        "#226: the handler returns from inside the scope.\nGot: {api_flat}"
    );

    assert!(db_code.contains("pub fn __rows_by_status(&self, value: &str) -> Option<Vec<usize>>"),
        "#160 C/#228: indexed field resolves candidate rows");
    assert!(db_code.contains("pub fn __rows_by_email(&self, value: &str) -> Option<Vec<usize>>"),
        "#160 C/#228: unique-indexed field resolves candidate rows");
    assert!(api_code.contains("db.user.__rows_by_status(__v)"),
        "#160 C: live list tries index pushdown");
    assert!(
        api_flat.contains("} else { None }; return db .user .__with_page( __sel,"),
        "#160 C: a parse-failure falls back to the full scan (never misses a match), \
         and the resolved selection feeds the page call directly.\nGot: {api_flat}"
    );
    assert!(
        api_flat.contains(
            "let __keep_all: bool = __user_is_unfiltered(&params); \
             if __keep_all && qp.sort.is_none() {"
        ),
        "#288/#281: the predicate is hoisted out of the per-row loop AND gates the \
         fast page, both before the selection is resolved.\nGot: {api_flat}"
    );
    assert!(!db_code.contains("fn __rows_by_region("),
        "#160 C: a composite-only field is not a single-field pushdown");

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
    assert!(scan_flat.contains("Some(mut __c) => { __c.sort_unstable(); __c }"),
        "#228: a pushdown selection is sorted for span locality.\nGot: {scan_flat}");
    assert!(scan.contains("for __slot in 0..__n"),
        "#168: buffered decode iterates slots");
    assert!(scan.contains("f(&mut __refs)"),
        "#228: the scope hands the borrowed views to the caller's callback");
}

#[test]
fn test_rust_generation_ordered_index() {
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

    assert!(db_code.contains("views_ordered"), "#169: u64 gets an ordered index");
    assert!(db_code.contains("BTreeMap<u64, std :: collections :: BTreeSet")
            || db_code.contains("BTreeMap<u64,"),
        "#169: ordered index keyed by the typed value (u64), not a String");
    assert!(db_code.contains("pub fn find_by_views_range"), "#169: u64 range/top-N method");
    assert!(db_code.contains("pub fn find_by_price_range"), "#169: decimal range method");
    assert!(db_code.contains("pub fn find_by_at_range"), "#169: timestamp range method");
    assert!(db_code.contains("price_ordered"), "#169: decimal ordered index");
    let price_range = &db_code[db_code.find("fn find_by_price_range").unwrap()..];
    let price_range = &price_range[..price_range.find("__out\n").unwrap_or(price_range.len().min(1200))];
    assert!(price_range.contains("normalize"),
        "#169: decimal range bounds normalized to match the stored key");

    assert!(db_code.contains("views_index"), "#169: hash index kept alongside (parallel, not replace)");
    assert!(db_code.contains("pub fn find_by_views"), "#169: exact-match probe still emitted");

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

    assert!(!db_code.contains("name_ordered"), "#169: string is exact-match only");
    assert!(!db_code.contains("find_by_name_range"), "#169: no range on a string index");
    assert!(!db_code.contains("score_ordered"), "#169: nullable ordered field deferred");
    assert!(!db_code.contains("find_by_score_range"), "#169: no range on a nullable field");
}

#[test]
fn test_api_openapi_doc_structure() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

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

    println!("Generated code:\n{}", code);

    assert!(code.contains("pub name: [u8; 200usize]"), "Missing: pub name: [u8; 200usize]");
    assert!(code.contains("pub address: Address"), "Missing: pub address: Address");
    assert!(code.contains("pub location: Option<Location>"), "Missing: pub location: Option<Location>");
    assert!(code.contains("pub tags: [[u8; 20usize]; 5usize]"), "Missing: pub tags: [[u8; 20usize]; 5usize]");
    assert!(code.contains("pub scores: [f64; 10usize]"), "Missing: pub scores: [f64; 10usize]");

    assert!(code.contains("name_col"), "Missing: name_col");
    assert!(code.contains("address_col"), "Missing: address_col");
    assert!(code.contains("location_col"), "Missing: location_col");
    assert!(code.contains("tags_col"), "Missing: tags_col");
    assert!(code.contains("scores_col"), "Missing: scores_col");
}

#[test]
fn test_rust_generation_codegen_gaps() {
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

    assert!(code.contains("pub struct GeoPoint"), "struct definition must be emitted");
    assert!(code.contains("pub latitude: f64"), "struct fields must be emitted");

    assert!(
        code.contains("id_to_row: std::sync::Arc<HashMap<u64, usize>>"),
        "id map must be keyed by u64 (Arc-wrapped, #158)"
    );
    assert!(code.contains("-> Result<u64, ValidationError>"), "insert must return the u64 PK");
    assert!(code.contains("id: u64"), "get must take the u64 PK");

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

    assert!(code.contains("pub fn all(&self) -> Vec<Post>"), "scan helper");

    assert!(
        code.contains("pub fn post_author(&self, record: &Post) -> Option<User>"),
        "forward required FK getter"
    );
    assert!(
        code.contains("record.editor.and_then(|fk| self.user.get(fk))"),
        "forward optional FK getter uses and_then"
    );

    assert!(
        code.contains("pub fn user_posts_by_author(&self, id: Uuid) -> Vec<Post>"),
        "reverse getter disambiguated by required FK"
    );
    assert!(
        code.contains("pub fn user_posts_by_editor(&self, id: Uuid) -> Vec<Post>"),
        "reverse getter disambiguated by optional FK"
    );
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

    assert!(code.contains("pub struct PostWithRelations"), "eager-load struct");
    assert!(
        code.contains("pub fn post_with_relations(&self, id: Uuid) -> Option<PostWithRelations>"),
        "eager-load getter"
    );

    insta::assert_snapshot!(code);
}

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

    assert!(code.contains("pub author_id: Uuid"), "author_id should be Uuid");
    assert!(code.contains("pub editor_id: Option<Uuid>"), "editor_id should be Option<Uuid>");

    assert!(code.contains("author_id_col: FixedColumn"), "author_id must have a storage column");
    assert!(code.contains("editor_id_col: FixedColumn"), "editor_id must have a storage column");

    assert!(!code.contains("author_id: Default::default()"), "author_id must not use default (silent data loss)");
    assert!(!code.contains("editor_id: None,"), "editor_id must not use None default (silent data loss)");

    assert!(code.contains("\"post/fixed/"), "Post paths not namespaced");
    assert!(code.contains("\"author/fixed/"), "Author paths not namespaced");

    assert!(code.contains("#[repr(C)]"), "missing #[repr(C)]");

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

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_with_component_field() {
    let schema = component_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    let code = &result.code;

    assert!(code.contains("reviews: ()"), "OneToMany should default to ()");
    assert!(code.contains("card: Default::default()"), "Component should default");

    insta::assert_snapshot!(code);
}

#[test]
fn test_typescript_generation_snapshot() {
    let schema = fk_schema();
    let result = TypeScriptGenerator::generate(&schema).unwrap();
    let code = &result.code;

    assert!(
        code.contains("${encodeURIComponent(id)}"),
        "id interpolation missing in template literal"
    );
    assert!(!code.contains("{}`"), "old malformed URL pattern still present");
    assert!(
        code.contains("/api/post/${encodeURIComponent(id)}"),
        "get URL for Post should be /api/post/${{encodeURIComponent(id)}}"
    );
    assert!(
        code.contains("/api/author/${encodeURIComponent(id)}"),
        "get URL for Author should be /api/author/${{encodeURIComponent(id)}}"
    );
    assert!(code.contains("id: string"), "Uuid should be string");
    assert!(code.contains("async updatePost("), "SDK should expose update");
    assert!(code.contains("async deletePost("), "SDK should expose delete");
    assert!(code.contains("export class ForgeDBError"), "SDK should define a typed error");
    assert!(code.contains("ListResult<Post>"), "list should return a paginated result");
    assert!(code.contains("export type PostCreate"), "SDK should expose a create-input type");

    insta::assert_snapshot!(code);
}

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
    assert!(result.code.contains("count_u32: number"), "U32 should be number");
    assert!(result.code.contains("count_u64: number"), "U64 should be number");
}

#[test]
fn test_rust_generation_reopen_rehydration() {
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

    assert!(
        code.contains("let n = db.tombstones.len();"),
        "reopen must anchor row_count on the tombstone file length"
    );
    assert!(code.contains("db.row_count = n;"), "reopen must set row_count");

    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);"),
        "reopen must rebuild id_to_row (via Arc::make_mut, #158)"
    );
    assert!(
        code.contains("db.id_col.read_uuid(i)"),
        "uuid-PK reopen reads the id column via read_uuid"
    );
    assert!(
        code.contains("db.id_col.read_u64(i)"),
        "integer-PK reopen reads the id column via read_u64"
    );

    assert!(
        code.contains("let row_count = right_col.len();"),
        "junction reopen must rehydrate row_count from right_col length"
    );
}

#[test]
fn test_rust_generation_reopen_index_rebuild_is_narrow() {
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
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        !code.contains("if let Some(__rec) = db.get(__id)"),
        "reopen index rebuild must NOT decode the whole record via db.get()"
    );
    assert!(
        code.contains("let __row = match db.id_to_row.get(&__id)"),
        "reopen index rebuild must resolve the physical row from id_to_row"
    );
    assert!(
        code.contains("if db.tombstones.is_deleted(__row)"),
        "reopen index rebuild must gate on the tombstone before indexing"
    );
    assert!(
        flat.contains(".email_col .read_string(__row)"),
        "reopen must read the indexed email column at the resolved row"
    );
    assert!(
        flat.contains(".age_col .read_u32(__row)"),
        "reopen must read the indexed age column at the resolved row"
    );
    assert!(
        !flat.contains(".bio_col .read_string(__row)"),
        "reopen must NOT read the non-indexed bio column (partial hydrate)"
    );
}

#[test]
fn test_rust_generation_column_projection() {
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

    assert!(code.contains("pub struct UserCard"), "projection struct emitted");
    assert!(
        flat.contains("pub struct UserCard { pub id: Uuid, pub email: String, pub age: u32, }"),
        "UserCard = PK + selected only, tight types.\nGot: {flat}"
    );

    assert!(code.contains("pub fn read_card_at"), "read_card_at emitted");
    assert!(flat.contains(".id_col .read_uuid(row_index)"), "reads PK column");
    assert!(flat.contains(".email_col .read_string(row_index)"), "reads selected email column");
    assert!(flat.contains(".age_col .read_u32(row_index)"), "reads selected age column");

    let body_start = code.find("fn read_card_at").expect("has read_card_at");
    let body = &code[body_start..body_start + 800.min(code.len() - body_start)];
    assert!(!body.contains("bio_col"), "projection decoder must NOT read unselected bio column");
    assert!(!body.contains("created_at_col"), "projection decoder must NOT read unselected created_at column");
    assert!(body.contains("UserCard"), "projection decoder constructs the projection struct");

    assert!(code.contains("pub fn get_card"), "get_card emitted");
    assert!(code.contains("pub fn all_card"), "all_card emitted");
    assert!(code.contains("pub fn get_card_at"), "snapshot get_card_at emitted");
    assert!(code.contains("pub fn all_card_at"), "snapshot all_card_at emitted");
}

#[test]
fn test_rust_generation_projection_rejects_relation_field() {
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

    assert!(
        flat.contains("payload_col : VariableColumn") || flat.contains("payload_col: VariableColumn"),
        "payload uses a VariableColumn.\nGot: {flat}"
    );

    assert!(
        flat.contains("serde_json :: to_string") || flat.contains("serde_json::to_string"),
        "json write path serializes via serde_json::to_string.\nGot: {flat}"
    );

    assert!(
        flat.contains("serde_json :: from_str") || flat.contains("serde_json::from_str"),
        "json read path decodes via serde_json::from_str.\nGot: {flat}"
    );
    assert!(
        flat.contains("payload_col .read_string") || flat.contains("payload_col.read_string"),
        "json read path reads the variable string column.\nGot: {flat}"
    );

    assert!(
        code.contains("append_tagged(1u8, &s)") && code.contains(r#"append_tagged(0u8, "")"#),
        "nullable json uses the presence-tag scheme"
    );
    assert!(
        code.contains("'\\u{1}'") || code.contains("as_bytes"),
        "nullable json read path must still split the presence tag"
    );
}

#[test]
fn test_rust_generation_layout_manifest() {
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

    assert!(
        code.contains("db.write_manifest(root);"),
        "model new_at() must refresh the root-scoped layout manifest on open"
    );
    assert!(
        code.contains("fn write_manifest(&self, root: &std::path::Path)"),
        "a root-scoped write_manifest method must be generated"
    );

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

    assert!(
        code.contains("\"tombstones.bin\"") && code.contains("bytes_per_row : 1usize")
            || code.contains("bytes_per_row: 1usize"),
        "model manifest must anchor row count on tombstones.bin at 1 byte/row"
    );
    assert!(
        code.contains("root.join(\"post/manifest.json\")")
            || code.contains("root.join (\"post/manifest.json\")"),
        "model manifest path must be the root-joined model directory"
    );
    assert!(
        code.contains("save_to(&__manifest_abs)") || code.contains("save_to (& __manifest_abs)"),
        "model manifest must be written via the bound __manifest_abs path"
    );

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
    let src = r#"
Post {
  id: +uuid
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

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

    assert!(
        code.contains("fn bump_compaction_epoch"),
        "a bump_compaction_epoch method must be generated"
    );
    assert!(
        code.contains("saturating_add (1)") || code.contains("saturating_add(1)"),
        "the epoch bump must increment by 1"
    );

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

    assert!(
        code.contains("fn read_at(&self, row_index: usize) -> Option<Post>"),
        "a shared read_at(row_index) accessor must be generated"
    );
    assert!(
        code.contains("let row_index = *self.id_to_row.get(&id)?;")
            && code.contains("self.read_at(row_index)"),
        "get must resolve the id then delegate to read_at"
    );

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
    assert!(
        !code.contains("let mut newest: Option<usize> = None;"),
        "#159: the O(watermark) get_at scan is replaced by a binary search"
    );
    assert!(
        code.contains("self.pairs_prefix(snap.watermark())")
            && code.contains("let end = snap.watermark();"),
        "junction pairs_at must resolve latest-wins over the committed prefix"
    );

    assert!(
        code.contains("pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)>")
            || code.contains("pub fn pairs_at(&self, snap: &forgedb_storage :: Snapshot) -> Vec<(Uuid, Uuid)>"),
        "junction must expose a watermark-clamped pairs_at"
    );

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

    assert!(
        code.contains("if !self.id_to_row.contains_key(&id)"),
        "update must no-op on an absent id"
    );

    assert!(
        code.contains("let record = match self.get(id)"),
        "delete resolves the current record"
    );
    assert!(
        code.contains("let deleted_row = *self")
            && code.contains("self.tombstones.append(true)"),
        "delete appends a tombstoned superseding version"
    );

    assert!(
        !code.contains("write_all_at") && !code.contains("write_at"),
        "mutation must be append-only — no in-place positional writes"
    );
}

#[test]
fn test_rust_generation_durable_write_path() {
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

    assert!(
        code.contains("wal: forgedb_wal::WalManager"),
        "storage holds a WAL handle"
    );
    assert!(
        code.contains("forgedb_wal::WalManager::open")
            && code.contains("forgedb_wal::FsyncPolicy::Always"),
        "WAL opened with fsync-on-commit durability"
    );

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

    assert!(
        code.contains("serde_json::to_vec(&record)") && code.contains(".write(&forgedb_wal::WalEntry::raw"),
        "mutations serialize the row and write it to the WAL before appends"
    );

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

    assert!(
        code.contains("const WAL_CHECKPOINT_INTERVAL: u64"),
        "generated checkpoint interval constant"
    );
    assert!(
        code.contains("writes_since_checkpoint: u64"),
        "storage tracks mutations since the last checkpoint"
    );

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
    assert!(
        code.matches(".sync_to_drive()").count() > code.matches(".barrier()").count(),
        "each checkpoint syncs many columns to drive but issues a single barrier (#153)"
    );
    let sync_pos = code.find("Failed to sync tombstones to drive on checkpoint");
    let barrier_pos = code.find("Failed to issue checkpoint device barrier");
    let trunc_pos = code.find("Failed to truncate WAL on checkpoint");
    assert!(
        matches!((sync_pos, barrier_pos, trunc_pos), (Some(s), Some(b), Some(t)) if s < b && b < t),
        "sync-to-drive, then the barrier, then WAL truncate (durability ordering, #153)"
    );

    assert!(
        code.contains("self.writes_since_checkpoint += 1;")
            && code.contains("if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL"),
        "mutations count toward and trigger the auto-checkpoint"
    );

    assert!(
        code.contains("self.user.checkpoint();") && code.contains("self.tag.checkpoint();"),
        "Database::checkpoint() checkpoints every model collection"
    );

    assert!(
        code.contains("last_checkpoint: self.row_count as u64"),
        "manifest last_checkpoint reflects the durable row count (observability)"
    );
    assert!(
        !code.contains("last_checkpoint: 0"),
        "no hardcoded-0 checkpoint left in the manifest"
    );

    assert!(
        code.contains("Failed to sync junction left column on checkpoint")
            && code.contains("Failed to issue junction checkpoint device barrier"),
        "junction checkpoint syncs its id columns + one barrier (no WAL to truncate)"
    );
}

#[test]
fn test_rust_generation_version_guard() {
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

    assert!(
        code.contains("const EXPECTED_SCHEMA_VERSION: u32 = 1"),
        "generated app bakes in the version it expects"
    );

    assert!(
        code.contains("__m.schema_version != EXPECTED_SCHEMA_VERSION"),
        "open compares the manifest schema serial against the expected version"
    );
    assert!(
        code.contains("but this binary expects v"),
        "the mismatch panic fails fast with migration guidance"
    );

    assert!(
        code.contains("root.join(\"user/manifest.json\")")
            && code.contains("root.join(\"tag/manifest.json\")")
            && code.contains("root.join(\"tag_user_link/manifest.json\")"),
        "the guard covers every model and junction manifest"
    );

    assert!(
        !code.contains("__m.columns") && !code.contains("m.column_type"),
        "the version guard reads no column shape (never self-heals — DV-6)"
    );

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
    let src = r#"
User {
  id: +uuid
  email: &string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

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
    assert!(
        !code.contains("schema_version: 1,"),
        "no hardcoded schema version left to clobber a bumped version on reopen"
    );
}

#[test]
fn test_rust_generation_secondary_indexes() {
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

    assert!(
        code.contains("email_index: std::sync::Arc<"),
        "unique field gets a value->ids index (Arc-wrapped)"
    );
    assert!(
        code.contains("handle_index") ,
        "^index field gets an index too"
    );
    assert!(!code.contains("name_index"), "plain fields are not indexed");

    assert!(code.contains("pub fn get_by_email(&self, value: &str) -> Option<User>"),
        "unique -> get_by_ Option probe");
    assert!(code.contains("pub fn find_by_email(&self, value: &str) -> Vec<User>"),
        "unique also exposes find_by_ Vec probe");
    assert!(code.contains("pub fn find_by_handle(&self, value: &str) -> Vec<User>"),
        "^index -> find_by_ Vec probe");

    assert!(
        code.contains("match self.email_index.get(&__k)"),
        "probe hits the index map, not a full scan"
    );

    assert!(
        code.contains("pub fn find_by_email_at(")
            && code.contains("self.get_at(snap, __id)"),
        "snapshot probe resolves candidates through the version-aware read path"
    );

    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.email_index)"),
        "insert/update/delete maintain the index (via Arc::make_mut)"
    );

    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.email_index)"),
        "indexes are rebuilt from committed rows on reopen (via Arc::make_mut)"
    );
}

#[test]
fn test_rust_generation_index_followups() {
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

    assert!(code.contains("handle_index"), "#102: nullable ^field is indexed");
    assert!(
        code.contains("pub fn find_by_handle(&self, value: Option<&str>) -> Vec<User>"),
        "#102: nullable string probe takes Option<&str>"
    );
    assert!(
        code.contains(r"String::from('\u{0}')"),
        "#102: None keys to a distinct null sentinel"
    );

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
    assert!(
        code.contains("self.post.find_by_author(id)"),
        "#100: reverse getter user_posts probes find_by_author (not a scan)"
    );

    assert!(
        code.contains("region_tier_index"),
        "#101: composite index field named <a>_<b>_index"
    );
    assert!(
        code.contains("pub fn find_by_region_and_tier(&self, region: &str, tier: u32) -> Vec<User>"),
        "#101: composite probe find_by_<a>_and_<b> with per-component params"
    );
    assert!(
        code.contains("__ck.push_str(&__p.len().to_string());") && code.contains("__ck.push(':');"),
        "#101: composite key is length-prefixed (collision-free join)"
    );

    assert!(
        code.contains("handle_index: self.handle_index.clone()"),
        "#103: reader clones the index maps"
    );
    assert!(
        code.contains("region_tier_index: self.region_tier_index.clone()"),
        "#103: reader clones the composite index too"
    );
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
    assert!(
        code.contains("status_index: self.status_index.clone()"),
        "#158: reader captures the index Arc (cheap clone)"
    );

    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.status_index)"),
        "#158: index add/remove mutates via Arc::make_mut (copy-on-write)"
    );
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.id_to_row).insert("),
        "#158: id_to_row mutates via Arc::make_mut"
    );
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

    assert!(
        code.contains("id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>"),
        "#159: id_versions is an Arc<HashMap<Id, Vec<row>>>"
    );
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut self.id_versions)")
            && code.contains(".push(row_index);"),
        "#159: insert/update/delete push the new version index"
    );
    assert!(
        code.contains("std::sync::Arc::make_mut(&mut db.id_versions)")
            && code.contains(".push(i);"),
        "#159: reopen rebuilds id_versions in the id-scan"
    );
    assert!(
        code.contains("versions.partition_point(|&r| r < watermark)"),
        "#159: get_at/all_at binary-search the version list"
    );
}

#[test]
fn test_rust_generation_data_integrity() {
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

    assert!(code.contains("pub enum ValidationError"), "ValidationError type emitted");
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("Unique{model:&'staticstr,field:&'staticstr")
            && flat.contains("DanglingReference{model:&'staticstr,field:&'staticstr,target:&'staticstr")
            && flat.contains("Constraint{model:&'staticstr,field:&'staticstr,rule:&'staticstr,message:String"),
        "three integrity variants, each carrying (model, field)"
    );
    assert!(code.contains("pub fn status_code(&self) -> u16"), "status_code maps to HTTP");

    assert!(code.contains("fn validate_user(record: &User) -> Result<(), ValidationError>"),
        "generated per-model field validator");
    assert!(code.contains("rule: \"email\"") && code.contains("rule: \"min\"")
        && code.contains("rule: \"max\"") && code.contains("rule: \"length\""),
        "each declared directive is enforced");

    assert!(code.contains("pub fn insert(&mut self, record: User) -> Result<Uuid, ValidationError>"),
        "insert returns Result");
    assert!(code.contains("validate_user(&record)?;"), "insert/update call the validator first");
    assert!(code.contains("self.email_index.get(&__uk)") && code.contains("ValidationError::Unique"),
        "duplicate &unique email is rejected via the unique index");

    assert!(code.contains("pub fn create_post(&mut self, mut record: Post) -> Result<Uuid, ValidationError>"),
        "create_<model> wrapper");
    assert!(code.contains("pub fn update_post(") && code.contains("-> Result<bool, ValidationError>"),
        "update_<model> wrapper");
    assert!(code.contains("self.user.get(record.author).is_none()"),
        "required FK author existence checked");
    assert!(code.contains("if let Some(__fk) = record.reviewer"),
        "optional FK reviewer checked only when set");
    assert!(code.contains("ValidationError::DanglingReference"), "dangling FK rejected");
}

#[test]
fn test_rust_generation_numeric_bounds_per_domain() {
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

    assert!(
        flat.contains("(*__v)<rust_decimal::Decimal::from(1i64)"),
        "decimal @min compares in the decimal domain"
    );
    assert!(
        flat.contains("(*__v)>rust_decimal::Decimal::from(1000000i64)"),
        "decimal @max compares in the decimal domain"
    );
    assert!(
        flat.contains("(*__v)<rust_decimal::Decimal::from(0i64)"),
        "nullable decimal @min is enforced too"
    );

    assert!(
        flat.contains("(*__vasi128)<(9007199254740993i64asi128)"),
        "u64 @min compares as i128, not f64"
    );
    assert!(
        flat.contains("(*__vasi128)<(0i64asi128)"),
        "nullable i32 @min compares as i128"
    );

    assert!(
        flat.contains("(*__vasf64)>(1i64asf64)"),
        "f64 @max stays in the f64 domain"
    );

    assert_eq!(
        flat.matches("(*__vasf64)").count(),
        1,
        "only the f64 field compares through f64"
    );

    for field in ["price", "discount", "qty", "ratio", "age"] {
        assert!(
            code.contains(&format!("field: \"{field}\"")),
            "{field} emits a constraint check"
        );
    }
}

#[test]
fn test_rust_generation_fractional_and_exclusive_bounds() {
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
    assert!(
        !flat.contains("Decimal::from_str") && !flat.contains("0.01f64"),
        "a decimal bound never passes through a parse or an f64 literal"
    );

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

    assert!(flat.contains("(*__vasf64)<-273.15f64"), "f64 fractional bound");

    assert!(
        flat.contains("(*__vasi128)<(-273i64asi128)"),
        "negative integer bound"
    );

    assert!(code.contains(r#""must be >= 0.01""#), "inclusive message");
    assert!(code.contains(r#""must be > 0.00""#), "exclusive message");
    assert!(code.contains(r#""must be < 1""#), "exclusive max message");
    assert!(code.contains(r#""must be >= -273.15""#), "negative message");
}

#[test]
fn test_rust_generation_delete_restrict() {
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
    assert!(
        code.contains("let __children = self.post.find_by_author(id);")
            && code.contains("if !__children.is_empty()")
            && code.contains("model: \"Post\"")
            && code.contains("field: \"author\""),
        "restrict checks the referencing children and refuses"
    );
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
    assert!(
        code.contains("if __depth > MAX_CASCADE_DEPTH"),
        "cascade worker guards recursion depth (cycle safety)"
    );
}

#[test]
fn test_rust_generation_delete_set_null() {
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

    assert!(
        code.contains("tombstones: Tombstones,"),
        "junction gains a per-pair Tombstones column"
    );
    assert!(
        code.contains("self.tombstones.append(false)"),
        "link appends a live (false) tombstone"
    );
    assert!(
        code.contains("pub fn unlink(&mut self, left: Uuid, right: Uuid) -> bool")
            && code.contains("self.tombstones.append(true)"),
        "unlink appends a retracted (true) pair"
    );
    assert!(
        code.contains("pub fn unlink_post_tag(&mut self, left: Uuid, right: Uuid) -> bool"),
        "Database exposes unlink_<a>_<b>"
    );
    assert!(
        code.contains("fn pairs_prefix(&self, end: usize) -> Vec<(Uuid, Uuid)>")
            && code.contains(".filter(|pair| !state.get(pair).copied().unwrap_or(true))"),
        "pairs resolves latest-wins, excluding retracted pairs"
    );
    assert!(
        code.contains("self.post_tag_link.unlink_all_left(id);"),
        "cascade-delete unlinks the model's junction rows"
    );
}

#[test]
fn test_rust_generation_pattern_validation() {
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

    assert!(
        flat.contains("fn validate_account(record: &Account) -> Result<(), ValidationError>"),
        "generated per-model field validator"
    );
    assert!(flat.contains("rule: \"pattern\""), "@pattern/@regex map to the Constraint rule");

    assert!(
        flat.contains("std::sync::LazyLock<regex::Regex>"),
        "pattern compiled once into a LazyLock<regex::Regex> static"
    );
    assert!(flat.contains("regex::Regex::new(\"^[0-9]+$\")"), "@pattern source embedded");
    assert!(flat.contains("regex::Regex::new(\"^[a-z-]+$\")"), "@regex source embedded");
    assert!(flat.contains(".is_match(__v.as_str())"), "the value is tested with is_match");

    assert!(
        flat.contains("if let Some(__v) = &record.slug"),
        "nullable pattern field validated only when Some"
    );
}

#[test]
fn test_rust_generation_auto_compaction() {
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

    assert!(
        code.contains("compact_model_keeping"),
        "generated code uses the keep-set primitive"
    );
    assert!(
        !code.contains("BackgroundCompactor"),
        "generated code must NOT link the off-writer-lock background thread"
    );

    assert!(
        code.contains("for &__row in self.id_to_row.values()")
            && code.contains("self.tombstones.is_deleted(__row)"),
        "generated code computes the live keep-set from id_to_row + liveness"
    );

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

    assert!(
        code.contains("self.dead_since_compaction += 1;")
            && code.contains("if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD"),
        "update/delete count toward and trigger the auto-compaction"
    );

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

    assert!(
        code.contains("pub fn maintain(&mut self)"),
        "generated per-model + Database maintain() runs deferred compaction"
    );
    assert!(
        code.contains("self.widget.maintain();") && code.contains("self.gadget.maintain();"),
        "Database::maintain() runs deferred maintenance for every model"
    );

    assert!(
        code.contains("self.widget.compact();") && code.contains("self.gadget.compact();"),
        "Database::compact() compacts every model collection"
    );
}

#[test]
fn test_rust_generation_incremental_rehydrate() {
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

    assert!(
        code.contains("pub fn __reindex_delta(&mut self, from: usize)"),
        "incremental delta refresh method emitted"
    );
    assert!(
        code.contains("for __r in from..__n {"),
        "delta iterates only the new rows [from..row_count)"
    );
    let delta_start = code.find("pub fn __reindex_delta").unwrap();
    let delta_body = &code[delta_start..delta_start + 6000];
    assert!(
        delta_body.contains("if let Some(__old_rec) = self.get(id)"),
        "delta removes the superseded version's index keys via self.get (like update)"
    );
    assert!(
        delta_body.contains("if let Some(__new_rec) = self.read_at(__r)"),
        "delta adds the folded row's index keys via self.read_at (like insert/update)"
    );
    assert!(
        !delta_body.contains(".clear();"),
        "delta must not clear the maps (that is the full __reindex_committed path)"
    );

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
    assert!(
        compact_body.contains("let __saved_status_index = std::sync::Arc::clone(&self.status_index);")
            && compact_body.contains("self.status_index = __saved_status_index;"),
        "compact preserves the (renumber-invariant) index maps across the reopen"
    );

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

    assert!(
        code.contains("let __anchor = self.tombstones.len();"),
        "recovery anchors on the authoritative tombstone row count"
    );
    assert!(
        !code.contains(".min(self.") ,
        "recovery no longer truncates to the min column length (would wipe on a new field)"
    );

    assert!(
        code.contains("while self.note_col.len() < __anchor")
            && code.contains("while self.score_col.len() < __anchor"),
        "each column is backfilled up to the anchor when short"
    );

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

    assert!(
        code.contains("truncate_to_rows(__anchor)"),
        "torn/ahead columns truncate down to the anchor"
    );
}

#[test]
fn test_api_generation_list_endpoint() {
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

    assert!(!code.contains(r#"json!({ "data": [] })"#), "list stub is gone");
    assert!(
        code.contains("|r| __keep_all || __user_scan_matches(r, &params)"),
        "list filters the narrow scan via the generated closed-set matcher (no second \
         parser); #288 short-circuits it on the hoisted bool, same matcher"
    );
    assert!(
        code.contains("user_event_matches(r, &params)"),
        "as_of snapshot path reuses the full-record closed-set filter"
    );
    assert!(
        code.contains("forgedb_query_params::QueryParams::from_map")
            && code.contains("qp.pagination.apply(&rows)"),
        "query-params substrate parses + clamps pagination"
    );
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

    assert!(
        flat.contains("struct __ListEnvelope<'a, T: serde::Serialize> { data: &'a [T], total: usize, limit: usize, offset: usize, }"),
        "the envelope is a generic struct borrowing the page, fields in wire order"
    );
    assert_eq!(
        code.matches("struct __ListEnvelope").count(),
        1,
        "emitted once for the whole file, not per model"
    );

    assert!(
        flat.contains("Json(__ListEnvelope { data: &page, total, limit: qp.pagination.limit, offset: qp.pagination.offset, })"),
        "the list handler borrows the page into the envelope"
    );
    assert!(
        flat.contains("let __data: Vec<super::PostCard> = page .iter() .map(|r| super::PostCard {"),
        "the projection list arm collects a typed page"
    );
    assert!(
        flat.contains("Json(__ListEnvelope { data: &__data,"),
        "the projection list arm borrows its typed page into the same envelope"
    );

    assert!(
        flat.contains("async fn list_post( Query(params): Query<HashMap<String, String>>, State(db): State<Arc<RwLock<super::Database>>>, ) -> Response"),
        "the list handler returns Response"
    );
    assert!(
        flat.contains("async fn get_post( Path(id): Path<String>, Query(params): Query<HashMap<String, String>>, State(db): State<Arc<RwLock<super::Database>>>, ) -> Response"),
        "the get handler returns Response"
    );

    assert!(
        flat.contains("Some(record) => (StatusCode::OK, Json(record)).into_response()"),
        "the point read serializes the record directly"
    );
    assert!(
        flat.contains("Some(r) => (StatusCode::OK, Json(r)).into_response()"),
        "the projected point read serializes the projection struct directly"
    );

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

    assert!(
        code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed::ChangeKind::Inserted)")
            || code.contains("feed.emit(\"Post\", row_index, forgedb_changefeed :: ChangeKind :: Inserted)"),
        "Post insert must emit an Inserted signal"
    );
    assert!(
        code.contains("ChangeKind::Linked") || code.contains("ChangeKind :: Linked"),
        "M2M link must emit a Linked signal"
    );
    assert!(
        code.contains("\"post_tag_link\""),
        "the link emit carries the junction name, not a field"
    );

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

    assert!(code.contains("pub struct PostInserted"), "typed insert event struct");
    assert!(code.contains("pub struct PostUpdated"), "typed update event struct (#66)");
    assert!(code.contains("pub struct PostDeleted"), "typed delete event struct (#66)");
    assert!(
        code.contains("pub post: Post"),
        "the event struct carries the typed record"
    );

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

    assert!(
        code.contains("pub fn read_at(&self, row_index: usize) -> Option<Post>"),
        "read_at must be public for change-feed materialization"
    );
}

#[test]
fn test_rust_generation_replication_broker() {
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
    let code = RustGenerator::generate_with_config(&schema, 1, GenConfig::legacy_with_replication())
        .unwrap()
        .code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        flat.contains("broker:Option<")
            && flat.contains(
                "std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>"
            ),
        "each storage/Database must hold an optional shared durable broker"
    );
    assert!(flat.contains("fnattach_broker"), "storages must expose attach_broker");

    assert!(flat.contains(".record("), "mutations must record to the durable broker");
    assert!(
        flat.contains("serde_json::to_vec(&record).unwrap_or_default()"),
        "the broker is handed the opaque serialized row bytes (field-blind)"
    );
    for kind in ["Inserted", "Updated", "Deleted", "Linked"] {
        let needle = format!("forgedb_changefeed::ChangeKind::{}", kind);
        assert!(flat.contains(&needle), "broker record must carry ChangeKind::{}", kind);
    }
    assert!(
        flat.contains("left.as_bytes()") && flat.contains("right.as_bytes()"),
        "the link record carries the opaque left++right uuid bytes"
    );

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

    assert!(
        !flat.contains("matchmodel_name"),
        "the broker must never match on the model name to decode a field"
    );
}

#[test]
fn test_gen_config_knobs() {
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
    assert!(
        d.contains("self.dead_since_compaction>=COMPACTION_DEAD_THRESHOLD"),
        "default emits the auto-compaction trigger (#134 ON)"
    );

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
        web: true,
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
    assert!(
        !c.contains("self.dead_since_compaction>=COMPACTION_DEAD_THRESHOLD"),
        "compaction=false omits the auto-compaction trigger (#134 Tier A)"
    );
    assert!(
        c.contains("self.dead_since_compaction+=1"),
        "the dead-row counter stays live even with auto-compaction off (#134)"
    );
}

#[test]
fn test_rust_generation_replication_log_retention() {
    let src = r#"
Post {
  id: +uuid
  title: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let flatten = |code: &str| -> String { code.chars().filter(|c| !c.is_whitespace()).collect() };

    let off = flatten(&RustGenerator::generate_with_config(&schema, 1, GenConfig::legacy_with_replication()).unwrap().code);
    assert!(
        !off.contains(".prune_through("),
        "retention 0 emits no prune (#137 default byte-identical)"
    );

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

    assert!(
        flat.contains("let__record_json=serde_json::to_vec(&record)"),
        "the record is serialized once into __record_json (#157 A)"
    );
    assert!(
        flat.contains("__wal_payload.extend_from_slice(&__record_json)"),
        "the WAL reuses the shared serialized buffer (#157 A)"
    );
    assert!(
        flat.contains("ChangeKind::Inserted,__record_json,"),
        "the broker record moves the shared buffer, not a fresh serialize (#157 A)"
    );

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
    assert!(
        flat.contains("ifletSome(__old_rec)=&__old{"),
        "update groups removes under one old-record guard (#157 B)"
    );
}

#[test]
fn test_rust_generation_replica_apply_path() {
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

    assert!(flat.contains("pubenumApplyError"), "must emit an ApplyError type");
    assert!(
        flat.contains("implFrom<ValidationError>forApplyError"),
        "apply reuses the write path, so ValidationError converts into ApplyError"
    );

    assert!(
        flat.contains("pubfnapply(&mut self,kind:forgedb_changefeed::ChangeKind,bytes:&[u8],)")
            || flat.contains("pubfnapply(&mutself,kind:forgedb_changefeed::ChangeKind,bytes:&[u8],)"),
        "each id-bearing model must expose a follower apply(kind, bytes)"
    );
    assert!(
        flat.contains("serde_json::from_slice(bytes)"),
        "apply decodes the opaque row bytes into the typed record"
    );
    for m in ["self.insert(record)", "self.update(id,record)", "self.delete(id)"] {
        assert!(flat.contains(m), "apply must replay through the existing {m}");
    }

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

    assert!(
        flat.contains("pubfncommit(&mutself)->std::io::Result<()>"),
        "Database + each storage must expose an additive commit()"
    );

    assert!(
        !flat.contains("matchrecord."),
        "apply_frame/apply must never branch on a decoded record field"
    );
}

#[test]
fn test_rust_generation_recover_to() {
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

    assert!(
        flat.contains("pubfnrecover_to(&mutself,base_offset:u64,target_offset:u64,)->Result<u64,ApplyError>"),
        "Database must expose recover_to(base_offset, target_offset)"
    );
    assert!(
        flat.contains(".read_from(after,BATCH)"),
        "recover_to reads frames via DurableBroker::read_from"
    );
    assert!(
        flat.contains("self.apply_frame(ev)"),
        "recover_to must replay through the existing apply_frame, not a new path"
    );
    assert!(
        flat.contains("ifev.offset>target_offset"),
        "recover_to stops at the target offset (exclusive upper bound past target)"
    );
    assert!(
        flat.contains(".attach_broker(None)"),
        "recover_to detaches the broker during replay so apply doesn't re-record frames"
    );
    assert!(
        !flat.contains("matchrecord."),
        "recover_to must never branch on a decoded record field — offset is the only key"
    );
}

#[test]
fn test_wasm_generation_transport() {
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

    assert!(flat.contains("#[wasm_bindgen]"), "must annotate for wasm-bindgen");
    assert!(flat.contains("pubstructReplica"), "must expose a Replica struct");
    assert!(flat.contains("pubasyncfnopen("), "lifecycle: open");
    assert!(flat.contains("js_name=applyWire"), "lifecycle: applyWire");
    assert!(flat.contains("pubasyncfncommit("), "lifecycle: commit");
    assert!(flat.contains("pubfnwatermark(&self)->u64"), "lifecycle: watermark");
    assert!(
        flat.contains("apply_frame(&ev)"),
        "applyWire replays through the generated apply_frame"
    );

    for js in [
        "js_name=\"getUser\"", "js_name=\"userCount\"", "js_name=\"allUsers\"",
        "js_name=\"getPost\"", "js_name=\"postCount\"", "js_name=\"allPosts\"",
        "js_name=\"getTag\"", "js_name=\"tagCount\"", "js_name=\"allTags\"",
    ] {
        assert!(flat.contains(js), "missing core read {js}");
    }

    assert!(flat.contains("js_name=\"postAuthor\""), "forward FK getter");
    assert!(flat.contains("js_name=\"userPosts\""), "reverse one-to-many getter");
    assert!(flat.contains("js_name=\"userTags\""), "M2M forward query");
    assert!(flat.contains("js_name=\"tagUsers\""), "M2M reverse query");

    assert!(flat.contains(".post_author(&__rec)"), "forward FK resolves via generated getter");
    assert!(flat.contains(".user_posts(__pk)"), "reverse getter calls generated method");

    for forbidden in [".insert(", ".update(", ".delete(", "js_name=\"linkUserTag\"", "fnlink_"] {
        assert!(
            !flat.contains(forbidden),
            "transport must expose no mutator (found {forbidden})"
        );
    }
}

#[test]
fn test_wasm_generation_async_client_and_worker() {
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
    assert!(
        custom_worker.contains("replica[method]") && custom_worker.contains("scheduleCommit"),
        "the bootstrap stays the same schema-agnostic pipe (#148/#110)"
    );

    for name in [
        "getUser", "userCount", "allUsers", "getPost", "postCount", "allPosts",
        "getTag", "tagCount", "allTags", "postAuthor", "userPosts", "userTags", "tagUsers",
    ] {
        assert!(
            client.contains(&format!("async {name}(")),
            "client missing generated read {name}"
        );
    }

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

    for forbidden in ["async create", "async insert", "async update", "async delete", "async link"] {
        assert!(
            !client.contains(forbidden),
            "client must expose no mutator (found {forbidden})"
        );
    }

    assert!(
        worker.contains("replica[method](...args)"),
        "worker dispatches reads generically by method name"
    );
    for schema_token in ["User", "Post", "Tag", "email", "title", "author", "getUser", "userPosts"] {
        assert!(
            !worker.contains(schema_token),
            "worker bootstrap must be schema-agnostic (found `{schema_token}`)"
        );
    }
}

const SYM: &str = "blog_3f2a1b4c5d6e7f80_";

const SYM_B: &str = "blog_00112233445566ff_";

const FP: &str = "0123456789abcdef";

const CORE_PKG: &str = "blog-3f2a1b4c5d6e7f80-core";

#[test]
fn test_ffi_generation_spine() {
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
    let code = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        flat.contains("useforgedb_coreasdatabase;"),
        "spine links the app's `core` cache package, renamed so no generated byte carries the hash"
    );
    assert!(
        !flat.contains("moddatabase;"),
        "the old `mod database;` seam (a sibling database.rs copy) must be gone"
    );
    assert!(flat.contains("usedatabase::Database;"), "spine uses the generated Database");
    assert!(flat.contains("pubstructDb"), "opaque C-ABI Db handle");
    assert!(flat.contains("pubstructForgeError"), "C-ABI error object");

    for stem in [
        "version", "open", "close", "commit",
        "checkpoint", "compact", "error_code",
        "error_message", "error_free", "free_buffer",
    ] {
        assert!(
            flat.contains(&format!("fn{SYM}{stem}(")),
            "missing pinned ABI symbol {SYM}{stem}"
        );
        assert!(
            !flat.contains(&format!("fnforgedb_{stem}(")),
            "the constant `forgedb_` prefix is still being EXPORTED for {stem}"
        );
    }
    assert!(flat.contains("extern\"C\""), "symbols are extern \"C\"");
    assert!(flat.contains("no_mangle"), "symbols are #[no_mangle]");

    assert!(flat.contains("Database::open_at("), "open wraps the generated open_at");
    assert!(flat.contains(".inner.commit()"), "commit wraps the generated commit");
    assert!(flat.contains(".inner.checkpoint()"), "checkpoint wraps the generated checkpoint");
    assert!(flat.contains(".inner.compact()"), "compact wraps the generated compact");

    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

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
    let code = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    for model in ["user", "post", "reading"] {
        for op in ["insert", "get", "count", "all", "update", "delete", "get_at", "all_at"] {
            assert!(
                flat.contains(&format!("fn{SYM}{model}_{op}(")),
                "missing per-model op {SYM}{model}_{op}"
            );
        }
    }

    assert!(flat.contains(&format!("fn{SYM}snapshot(")), "snapshot capture entry point");
    assert!(flat.contains(&format!("fn{SYM}snapshot_free(")), "snapshot free entry point");
    assert!(flat.contains(".inner.snapshot()"), "capture wraps Database::snapshot()");

    assert!(flat.contains(".inner.create_user("), "insert uses the create_<m> integrity wrapper");
    assert!(flat.contains(".inner.update_post("), "update uses the update_<m> wrapper");
    assert!(flat.contains(".inner.delete_user("), "delete uses the delete_<m> referential wrapper");
    assert!(flat.contains(".inner.user.get("), "get uses the generated storage read");
    assert!(flat.contains(".inner.reading.row_count("), "count uses the generated row_count");

    assert!(flat.contains(".inner.user.get_at(&snap.inner.user,"), "get_at clamps to the model's watermark");
    assert!(flat.contains(".inner.post.all_at(&snap.inner.post)"), "all_at clamps to the model's watermark");

    assert!(flat.contains("database::User"), "records decode into the generated User struct");
    assert!(flat.contains("database::Post"), "records decode into the generated Post struct");
    assert!(flat.contains("serde_json::from_slice"), "opaque JSON bytes → typed record via serde");

    assert!(flat.contains("id:u64"), "integer-PK model decodes a u64 id (not a forced Uuid)");

    assert!(flat.contains("FORGEDB_ERR_VALIDATION"), "integrity failures map to a validation code");
    assert!(flat.contains("catch_unwind"), "per-model engine calls are catch_unwind-guarded");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "per-model ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_relation_ops() {
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
    let code = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(flat.contains(&format!("fn{SYM}post_author(")), "forward-FK getter post_author");
    assert!(flat.contains(".inner.post_author("), "wraps the generated post_author getter");
    assert!(flat.contains(".inner.post.get("), "forward-FK fetches the source record by id first");

    assert!(flat.contains(&format!("fn{SYM}user_posts(")), "reverse-1:M getter user_posts");
    assert!(flat.contains(".inner.user_posts("), "wraps the generated user_posts getter");

    assert!(flat.contains(&format!("fn{SYM}link_post_tag(")), "M2M link link_post_tag");
    assert!(flat.contains(&format!("fn{SYM}unlink_post_tag(")), "M2M unlink unlink_post_tag");
    assert!(flat.contains(&format!("fn{SYM}post_tags(")), "M2M forward query post_tags");
    assert!(flat.contains(&format!("fn{SYM}tag_posts(")), "M2M reverse query tag_posts");
    assert!(flat.contains(".inner.link_post_tag("), "wraps the generated link_post_tag");
    assert!(flat.contains(".inner.post_tags("), "wraps the generated post_tags query");

    assert!(flat.contains(&format!("fn{SYM}post_tags_at(")), "snapshot `_at` M2M traversal getter");
    assert!(flat.contains(".inner.post_tags_at(&snap.inner,"), "wraps the generated post_tags_at");
    assert!(
        !flat.contains(&format!("{SYM}tag_posts_at")),
        "only the forward M2M traversal is snapshot-scoped"
    );

    assert!(flat.contains("catch_unwind"), "traversal engine calls are catch_unwind-guarded");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "relation ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_async_ops() {
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
    let code = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(flat.contains(&format!("fn{SYM}set_completion_callback(")), "async completion-callback registration");
    assert!(flat.contains("typeForgeCompletion"), "the completion callback type is defined");
    assert!(flat.contains("staticCOMPLETION_CB"), "the process-wide callback slot exists");
    assert!(flat.contains("structSendDb"), "the Send db-pointer wrapper exists");
    assert!(flat.contains("unsafeimplSendforSendDb"), "SendDb is unsafe-Send for the worker thread");
    assert!(flat.contains("fnfire_completion("), "the completion delivery helper exists");
    assert!(flat.contains("fnasync_executor("), "the single background worker executor exists");
    assert!(flat.contains("fnspawn_async"), "jobs are enqueued on the worker");
    assert!(flat.contains("assert_send::<Db>("), "Db is statically asserted Send");

    for model in ["user", "post", "reading"] {
        for op in ["get", "all", "count", "insert", "update", "delete"] {
            assert!(
                flat.contains(&format!("fn{SYM}{model}_{op}_async(")),
                "missing async op {SYM}{model}_{op}_async"
            );
        }
    }
    assert!(flat.contains("token:u64"), "async ops carry the completion token");

    assert!(flat.contains(".inner.create_user("), "async insert uses the create_<m> integrity wrapper");
    assert!(flat.contains(".inner.update_post("), "async update uses the update_<m> wrapper");
    assert!(flat.contains(".inner.delete_user("), "async delete uses the delete_<m> referential wrapper");
    assert!(flat.contains(".inner.user.get(id)"), "async get uses the generated storage read");
    assert!(flat.contains(".inner.reading.row_count("), "async count uses the generated row_count");

    assert!(flat.contains("spawn_async(move||"), "async ops enqueue their engine call off the caller thread");
    assert!(flat.contains("catch_unwind"), "async engine calls are catch_unwind-guarded");

    assert!(flat.contains("FORGEDB_ERR_VALIDATION"), "async integrity failures map to a validation code");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "async ops must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_ffi_generation_arrow_export() {
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
    let code = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(flat.contains("structArrowSchema"), "the Arrow schema struct is defined");
    assert!(flat.contains("structArrowArray"), "the Arrow array struct is defined");
    assert!(flat.contains("structArrowArrayOwner"), "the export-buffer owner box is defined");
    assert!(flat.contains("fnarrow_array_release("), "the array release callback exists");
    assert!(flat.contains("fnarrow_schema_release("), "the schema release callback exists");
    assert!(flat.contains("fnfill_arrow_primitive("), "the fill helper exists");
    assert!(flat.contains("Box::from_raw"), "the array release reclaims + drops the owner box");
    assert!(flat.contains("n_buffers:2"), "a primitive array has two buffers (validity + data)");
    assert!(
        flat.contains("forgedb_core::forgedb_storage::ColumnExport"),
        "the export buffer is a ColumnExport reached through `core`'s re-export \
         (mmap alias or gathered copy)"
    );
    assert!(
        !flat.contains("_export:forgedb_storage::ColumnExport"),
        "the wrapper still pins substrate directly instead of routing through `core`"
    );

    for stem in [
        "user_id_export_arrow(",
        "user_age_export_arrow(",
        "user_score_export_arrow(",
        "user_created_at_export_arrow(",
        "post_id_export_arrow(",
        "post_views_export_arrow(",
        "post_author_export_arrow(",
    ] {
        assert!(
            flat.contains(&format!("fn{SYM}{stem}")),
            "missing Arrow export op {SYM}{stem}"
        );
    }

    assert!(flat.contains("w:16"), "uuid / FK columns export as Arrow FixedSizeBinary(16)");

    assert!(
        !flat.contains(&format!("{SYM}user_active_export_arrow")),
        "bool column must be skipped"
    );
    assert!(
        !flat.contains(&format!("{SYM}user_bio_export_arrow")),
        "nullable string column must be skipped"
    );

    assert!(flat.contains(".inner.user.export_live_indices()"), "the live row set comes from generated code");
    assert!(flat.contains(".inner.user.export_col_age("), "the column is exported via the generated export_col_<f>");
    assert!(flat.contains("fill_arrow_primitive("), "the exported buffer fills the Arrow structs");
    assert!(flat.contains("catch_unwind"), "the export is catch_unwind-guarded");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "Arrow export must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_pyo3_generation_binding() {
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

    assert!(flat.contains("structForgeDb"), "the ForgeDb handle exists");
    assert!(flat.contains("Database::open_at("), "open wraps Database::open_at");
    assert!(flat.contains("self.inner.commit()"), "commit wraps the generated commit");

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

    assert!(flat.contains("database::User"), "rows decode into the generated User struct");
    assert!(flat.contains("pythonize::depythonize"), "inbound rows/ids via pythonize");
    assert!(flat.contains("pythonize::pythonize"), "outbound rows/ids via pythonize");

    assert!(flat.contains("ForgeDbError"), "errors surface as a Python ForgeDbError");
    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

    assert!(flat.contains("#[pymodule]"), "a #[pymodule] entry point is generated");
    assert!(flat.contains("m.add_class::<ForgeDb>()"), "ForgeDb is registered");
    assert!(flat.contains("m.add_class::<PyUser>()"), "the User row class is registered");

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
    assert!(
        flat.contains("fnid(&self)->PyResult<u64>"),
        "u64 id getter returns u64"
    );
    assert!(
        flat.contains("fntitle(&self)->PyResult<String>"),
        "string title getter returns String"
    );
    assert!(
        flat.contains("fnauthor(&self)->PyResult<String>"),
        "required FK (Uuid) getter returns String"
    );
    assert!(flat.contains("fn__repr__"), "__repr__ is generated on every row class");
    assert!(flat.contains("fnto_dict"), "to_dict is generated on every row class");

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

    assert!(flat.contains("structArrowColumn"), "the ArrowColumn class exists");
    assert!(flat.contains("fn__arrow_c_array__"), "ArrowColumn implements the Arrow PyCapsule protocol");
    assert!(flat.contains("\"arrow_array\""), "the array capsule is named arrow_array");
    assert!(flat.contains("\"arrow_schema\""), "the schema capsule is named arrow_schema");
    assert!(flat.contains("m.add_class::<ArrowColumn>()"), "ArrowColumn is registered");
    assert!(
        flat.contains("fnreading_value_arrow(&self)->PyResult<ArrowColumn>"),
        "the exportable i64 column gets a zero-copy Arrow method"
    );
    assert!(flat.contains("fnreading_id_arrow(&self)->PyResult<ArrowColumn>"), "the u64 PK column is exportable");
    assert!(
        flat.contains("self.inner.reading.export_live_indices()") && flat.contains(".export_col_value("),
        "the export computes the live set in generated code + gathers the one column"
    );
    assert!(!flat.contains("post_title_arrow"), "a string column is not Arrow-exportable");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "the PyO3 binding must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_napi_generation_binding() {
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

    assert!(
        flat.contains("#[napi(js_name=\"ForgeDb\")]"),
        "the ForgeDb #[napi] class exists"
    );
    assert!(flat.contains("structForgeDb"), "the ForgeDb handle exists");
    assert!(flat.contains("Database::open_at("), "open wraps Database::open_at");
    assert!(flat.contains("self.write().commit()"), "commit wraps the generated commit under the write guard");
    assert!(flat.contains("#[napi(factory)]"), "open is a #[napi] factory");

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
    assert!(flat.contains("pubid:String"), "User id field is typed String (uuid)");
    assert!(flat.contains("pubvalue:i64"), "Reading value field is typed i64");
    assert!(
        flat.contains("#[napi(js_name=\"view_count\")]"),
        "row-struct fields pin their snake_case JS key (Post.view_count stays snake_case, not viewCount)"
    );

    assert!(
        flat.contains("->Result<Option<NapiUser>>"),
        "get_user returns typed Option<NapiUser>"
    );
    assert!(
        flat.contains("->Result<Vec<NapiPost>>") || flat.contains("->Result<Vec<NapiUser>>"),
        "all_<m> returns typed Vec<Napi<Model>>"
    );

    assert!(flat.contains("database::User"), "rows decode into the generated User struct");
    assert!(flat.contains("env.from_js_value"), "inbound rows/ids via Env::from_js_value");
    assert!(flat.contains("env.to_js_value"), "outbound ids (create) via Env::to_js_value");

    assert!(flat.contains("Error::from_reason"), "errors surface as a thrown JS Error");
    assert!(flat.contains("catch_unwind"), "engine calls are catch_unwind-guarded");

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
    assert!(!flat.contains("post_title_arrow"), "a string column is not Arrow-exportable");

    for forbidden in ["forgedb_query", "match model", "matchmodel", "predicate", "orderBy"] {
        assert!(
            !flat.contains(forbidden),
            "the NAPI-RS binding must invent no generic query surface (found `{forbidden}`)"
        );
    }
}

#[test]
fn test_api_generation_replication_endpoint() {
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

    assert!(
        code.contains("async fn __replicate") && code.contains("async fn __handle_replicate"),
        "the replication upgrade + handler are generated"
    );
    assert!(
        norm.contains(".route(\"/replicate\", get(__replicate))")
            || code.contains("/replicate"),
        "the /replicate route is registered"
    );

    assert!(
        code.contains("\"after\""),
        "the handler resumes from the ?after=<offset> query param"
    );
    assert!(
        norm.contains("catch_up_from(after, usize::MAX)"),
        "the handler uses the broker's race-free catch_up_from"
    );
    assert!(
        code.contains("ev.offset <= boundary"),
        "the live tail is idempotent by absolute offset (skip <= boundary)"
    );

    assert!(
        norm.contains("ev.to_wire()"),
        "frames are sent as opaque binary wire bytes"
    );
    assert!(
        code.contains("db.read().await.broker.clone()")
            || norm.contains("db.read().await.broker.clone()"),
        "the handler reads the shared broker from the Database"
    );

    let data_routes = code
        .split("fn __data_routes")
        .nth(1)
        .and_then(|s| s.split("fn __ops_routes").next())
        .unwrap_or("");
    assert!(
        data_routes.contains("/replicate"),
        "the /replicate route must be behind the tenant-auth guard (in __data_routes)"
    );

    assert!(
        !code.contains("match model_name"),
        "the replication transport must not decode a field by model name"
    );
}

#[test]
fn test_api_generation_websocket_subscription() {
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

    assert!(code.contains("WebSocketUpgrade"), "ws upgrade imported");
    assert!(
        code.contains("async fn subscribe_post"),
        "per-model subscription handler generated"
    );
    assert!(
        code.contains("/subscribe/"),
        "per-model /subscribe route registered"
    );

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
    assert!(
        code.contains(".post.read_at(event.row_index)"),
        "handler materializes the typed record from the row index"
    );

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
    assert!(
        code.contains("pub fn reader(&self) -> PostStorageReader"),
        "storage exposes a reader() handle"
    );
    assert!(
        code.contains(".reader().expect(\"Failed to open column reader\")"),
        "reader shares the writer's column fds via col.reader()"
    );

    assert!(
        code.contains("impl PostStorageReader"),
        "reader impl block generated"
    );
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

    assert!(code.contains("pub struct PostTagLinkReader"), "junction reader struct");
    assert!(
        code.contains("pub fn reader(&self) -> PostTagLinkReader"),
        "junction exposes a reader() handle"
    );

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
    assert!(
        !code.contains("fn read_at(&self, model: &str") && !code.contains("model_name: &str"),
        "no runtime model-name-keyed read dispatch"
    );

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
    assert!(
        code.contains("pub enum CounterLiveDelta") && code.contains("Removed { id: u64 }"),
        "integer-PK Removed carries the u64 id"
    );
}

#[test]
fn test_api_generation_live_query() {
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

    assert!(code.contains("/live-query/"), "per-model /live-query route registered");
    assert!(
        code.contains("async fn subscribe_live_post"),
        "per-model live-query handler generated"
    );

    assert!(
        code.contains("post_event_matches(r, &params)"),
        "live-query reuses the generated closed-set filter (no second predicate parser)"
    );
    assert!(
        code.contains(".post.all()") || code.contains(".post\n"),
        "live-query re-runs the generated all() query"
    );
    assert!(
        !code.contains("parse_predicate") && !code.contains("__gt") && !code.contains("__like"),
        "no operator grammar / predicate-as-data parser"
    );

    assert!(
        code.contains("if event.model != \"Post\""),
        "live-query re-runs on the coarse model signal"
    );

    assert!(
        code.contains("PostLiveDelta::Init") || code.contains("PostLiveDelta :: Init"),
        "handler streams the generated typed delta enum"
    );
    assert!(
        code.contains("PostLiveDelta::Removed") || code.contains("PostLiveDelta :: Removed"),
        "handler emits removal-aware deltas"
    );
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
    assert!(
        !code.contains("params.get(\"author\")"),
        "relation field is not a live-query filter key"
    );
}

#[test]
fn test_api_generation_typed_event_filter() {
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
        code.contains(".parse::<forgedb_types::Timestamp>()"),
        "#254: a timestamp filter parses the RFC 3339 wire form, the same form the \
         body uses — parsing a bare integer here would have silently meant SECONDS \
         against microsecond storage, matching nothing instead of failing"
    );
    assert!(
        code.contains("floor_to_micros(1000i64)"),
        "#389: `made_at` is bare `timestamp` (quantum 1ms), and the write path floors \
         the stored value to it. `Timestamp`'s PartialEq is over raw i64 micros and so \
         does NOT self-correct — unlike Decimal, whose PartialEq compares by numeric \
         value and needed only its key normalized. Without flooring the parsed param, \
         filtering by the instant you just wrote returns an empty page"
    );
    assert!(
        code.contains("serde_json::from_value::<") && code.contains("super::Status"),
        "enum filter reuses the canonical variant-name serde mapping"
    );

    assert!(
        !code.contains("serde_json::to_value(record)")
            && !code.contains("other.to_string() == *want"),
        "no stringify-compare remains in the event filter"
    );

    assert!(
        code.contains("fn widget_record_changed"),
        "generated typed change detector"
    );
    assert!(
        code.contains(".to_bits() != ") ,
        "f64 change-detection compares bit patterns (deterministic, NaN-stable)"
    );

    assert!(
        code.contains("params.get(\"status\")"),
        "enum field is a filter key"
    );
}

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

#[test]
fn test_openapi_generation_is_valid_document() {
    let schema = fk_schema();
    let code = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&code).expect("output is valid JSON");

    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["info"]["title"].is_string());
    assert!(spec["servers"].is_array());

    let paths = &spec["paths"];
    assert!(paths["/api/post"]["get"].is_object(), "list route");
    assert!(paths["/api/post"]["post"].is_object(), "create route");
    let item = &paths["/api/post/{id}"];
    assert!(item["get"].is_object(), "get-by-id route");
    assert!(item["put"].is_object(), "replace route");
    assert!(item["delete"].is_object(), "delete route");
    assert_eq!(item["parameters"][0]["name"], "id");
    assert_eq!(item["parameters"][0]["in"], "path");

    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("components.schemas is an object");
    assert!(schemas.contains_key("Author"));
    assert!(schemas.contains_key("Post"));

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

#[test]
fn test_openapi_generation_skips_virtual_fields() {
    let schema = component_schema();
    let code = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&code).unwrap();

    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for (_name, model) in schemas {
        if let Some(props) = model["properties"].as_object() {
            for (_field, prop) in props {
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

    assert!(
        code.contains(".append_uuid(record.price.serialize())"),
        "non-null decimal appends via Decimal::serialize() -> [u8; 16]"
    );
    assert!(
        code.contains("rust_decimal::Decimal::deserialize(bytes)"),
        "non-null decimal reads via Decimal::deserialize([u8; 16])"
    );
    assert!(
        code.contains("price_col: FixedColumn"),
        "decimal occupies a fixed column"
    );
    assert!(
        code.contains("std::mem::size_of::<Option<rust_decimal::Decimal>>()"),
        "nullable decimal sizes as Option<Decimal>"
    );

    assert!(code.contains("price_index"), "^decimal is indexed");
    assert!(
        code.contains("(record.price).normalize()"),
        "the decimal index key normalizes away scale (1.0 == 1.00)"
    );

    let api = ApiGenerator::generate(&schema).unwrap().code;
    assert!(
        api.contains("\"price\" => rows.sort_by(|a, b| a.price.cmp(&b.price))"),
        "decimal sorts via Ord::cmp, not partial_cmp"
    );

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_enum_type() {
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

    assert!(code.contains("pub enum OrderStatus"), "enum def emitted");
    assert!(
        code.contains("Copy") && code.contains("Ord") && code.contains("Hash"),
        "enum derives include Copy/Ord/Hash (needed for index + sort)"
    );

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
    assert!(
        code.contains("Some(v) => [1u8, v.__to_u8()]"),
        "nullable enum encodes a presence tag + discriminant"
    );
    assert!(
        code.contains("Some(OrderStatus::__from_u8(bytes[1]))"),
        "nullable enum decodes present values via __from_u8"
    );

    assert!(code.contains("status_index"), "^enum field is indexed");
    assert!(
        code.contains("pub fn find_by_status(&self, value: OrderStatus)"),
        "enum probe takes the enum type"
    );

    let api = ApiGenerator::generate(&schema).unwrap().code;
    assert!(
        api.contains("\"status\" => rows.sort_by(|a, b| a.status.cmp(&b.status))"),
        "enum sorts via Ord::cmp"
    );

    let ts = TypeScriptGenerator::generate(&schema).unwrap().code;
    assert!(
        ts.contains("export type OrderStatus = \"Pending\" | \"Paid\" | \"Shipped\";"),
        "TS emits a string-union alias for the enum"
    );
    assert!(
        ts.contains("status: OrderStatus;"),
        "TS field is typed as the enum union"
    );

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

    assert!(
        code.contains("forgedb_storage::Snapshot::new(self.db.user.row_count)")
            && code.contains("self.db.user.get_at(&__snap, id)"),
        "txn reads resolve via get_at at the raised (staged) watermark"
    );

    assert!(
        code.contains("pub fn __stage_append(&mut self, record: User, deleted: bool) -> usize"),
        "generated low-level staged-append that skips index/feed/broker"
    );
    assert!(
        code.contains("self.db.user.__stage_append(record, false)"),
        "TxHandle::create stages via __stage_append"
    );

    assert!(
        code.contains("fn rollback_internal(&mut self)")
            && code.contains("__truncate_all_to(__mark)")
            && code.contains("wal.truncate_to("),
        "rollback truncates staged rows + the staged WAL tail back to the mark"
    );
    assert!(
        code.contains("impl<'db> Drop for TxHandle<'db>")
            && code.contains("self.rollback_internal();"),
        "an un-committed TxHandle rolls back on drop"
    );

    assert!(
        code.contains("self.db.user.__reindex_committed();"),
        "commit advances visibility by rebuilding id_to_row + indexes"
    );
    assert!(
        code.contains("pending_events"),
        "changefeed/broker events are buffered and drained on commit"
    );

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

    assert!(
        !code.contains("match record.") && !code.contains("match field_name"),
        "transaction machinery must never match on a decoded field"
    );

    fn bodies_of<'a>(code: &'a str, decl: &str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(decl) {
            let start = from + rel;
            let tail = &code[start..];
            let end = tail[1..]
                .find("\n    pub fn ")
                .map(|i| i + 1)
                .unwrap_or_else(|| {
                    panic!("#170: no following `pub fn` bounds `{decl}` at byte {start}")
                });
            out.push(&tail[..end]);
            from = start + 1;
        }
        assert!(!out.is_empty(), "#170: `{decl}` is not emitted at all");
        out
    }

    let stage_bodies = bodies_of(&code, "fn __stage_append");
    assert_eq!(
        stage_bodies.len(),
        2,
        "#170: one __stage_append per model (User, Post)"
    );
    for (i, stage_body) in stage_bodies.iter().enumerate() {
        assert!(
            stage_body.contains(".write_buffered("),
            "#170: __stage_append #{i} uses the buffered (no-fsync) WAL append"
        );
        assert_eq!(
            stage_body.matches(".write(&forgedb_wal::WalEntry").count(),
            0,
            "#170: __stage_append #{i} does NOT per-record fsync (no plain wal.write)"
        );
    }

    let insert_bodies = bodies_of(&code, "pub fn insert(");
    assert_eq!(insert_bodies.len(), 2, "#170: one insert per model (User, Post)");
    for (i, insert_body) in insert_bodies.iter().enumerate() {
        assert_eq!(
            insert_body.matches(".write(&forgedb_wal::WalEntry").count(),
            1,
            "#170: insert #{i} still fsyncs per op — exactly one WAL write, inside its own body"
        );
    }
}

#[test]
fn test_rust_generation_txn_intra_unique() {
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

    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.matches("staged_unique_keys.contains(").count() >= 2,
        "both &unique fields (email, username) have a staged-buffer contains-check"
    );
    assert!(
        flat.matches("staged_unique_keys.insert(").count() >= 2,
        "both &unique fields claim their key in the buffer on success"
    );

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

    for model in ["Widget", "Gadget"] {
        let claim = format!("staged_unique_keys.insert(({model:?}, \"code\"");
        assert!(
            code.contains(&claim),
            "{model}.code must claim its staged key under its own model tag, not bare `code`"
        );
    }
    assert!(
        !code.contains("staged_unique_keys.insert((\"code\","),
        "no staged claim may be keyed by field name alone (#257)"
    );
    assert!(
        !code.contains("staged_unique_keys.contains(&(\"code\","),
        "no staged lookup may be keyed by field name alone (#257)"
    );

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

fn mentions_ident(code: &str, ident: &str) -> bool {
    let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    code.match_indices(ident).any(|(i, _)| {
        boundary(code[..i].chars().next_back())
            && boundary(code[i + ident.len()..].chars().next())
    })
}

#[test]
fn test_rust_generation_modifiers_on_non_identity_auto_fields() {
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

    assert!(
        mentions_ident(&code, "created_at_index"),
        "`^` on a non-identity auto field must build an index (#258)"
    );
    assert!(
        mentions_ident(&code, "ref_id_index"),
        "`&` on a non-identity auto field must build an index (#258)"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("ValidationError::Unique{model:\"Event\",field:\"ref_id\""),
        "`&` on a non-identity auto field must enforce uniqueness (#258)"
    );

    assert!(
        !mentions_ident(&code, "id_index"),
        "the identity field must NOT get a redundant secondary index (#258)"
    );
}

#[test]
fn test_rust_generation_identity_modifiers_stay_redundant() {
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

    assert!(
        code.contains("_txn_journal: Option<forgedb_wal::WalManager>"),
        "Database owns the transaction commit journal"
    );
    assert!(
        code.contains("root.join(\"_txn_journal.log\")"),
        "journal is a root-level append-only log"
    );

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

    assert!(
        code.contains("pub fn __recover_to_committed(&mut self, committed_len: usize)"),
        "generated per-model journal recovery"
    );
    assert!(
        code.contains("__db.user.__recover_to_committed(__len as usize)"),
        "open_at applies journalled committed lengths to each touched model"
    );

    assert!(
        code.contains("serde_json::to_vec(&__journal)"),
        "the journal record is opaque encoded bytes (length vector), not a decoded field"
    );
}

#[test]
fn test_rust_generation_txn_defers_maintenance() {
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

    assert!(
        code.contains("in_transaction: bool")
            && code.contains("checkpoint_deferred: bool")
            && code.contains("compact_deferred: bool"),
        "storage carries the txn guard + deferred flags"
    );

    assert!(
        code.contains("if self.in_transaction {")
            && code.contains("self.checkpoint_deferred = true;"),
        "auto-checkpoint defers to checkpoint_deferred while in a transaction"
    );
    assert!(
        code.contains("self.compact_deferred = true;"),
        "auto-compaction defers to compact_deferred while in a transaction"
    );
    let compact_idx = code.find("pub fn compact(&mut self) {").unwrap();
    let compact_body = &code[compact_idx..compact_idx + 400];
    assert!(
        compact_body.contains("if self.in_transaction {")
            && compact_body.contains("self.compact_deferred = true;")
            && compact_body.contains("return;"),
        "compact() early-returns (deferred) when called mid-transaction"
    );

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

    assert!(
        flat.contains("pubfntransaction_retrying<T>(")
            && flat.contains("retries:u32,")
            && flat.contains("implFn(&mutTxHandle)->Result<T,TxError>"),
        "Database::transaction_retrying is generated"
    );

    assert!(
        flat.contains("seq:std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>"),
        "Database carries the commit sequencer"
    );

    assert!(
        flat.contains("CommitSequencer::new(") && flat.contains("watermark()"),
        "sequencer seeded from broker watermark on open_at"
    );

    assert!(
        flat.contains(".try_commit(&__ws)"),
        "transaction_retrying calls try_commit on the sequencer"
    );

    assert!(
        flat.contains("fn__write_set(") || flat.contains("__write_set("),
        "TxHandle exposes __write_set to build the opaque write-set"
    );

    assert!(
        flat.contains("TxError::Conflict"),
        "exhausted retries return TxError::Conflict"
    );

    assert!(
        flat.contains("pubfntransaction_optimistic<T>("),
        "transaction_optimistic convenience wrapper is generated"
    );

    assert!(
        flat.contains("DEFAULT_TXN_RETRIES:u32"),
        "DEFAULT_TXN_RETRIES const is generated"
    );

    let retrying_idx = flat.find("transaction_retrying").expect("transaction_retrying in generated code");
    let retrying_body = &flat[retrying_idx..retrying_idx + 2000.min(flat.len() - retrying_idx)];
    assert!(
        !retrying_body.contains("matchmodel_name"),
        "the commit/conflict path must never match on the model name"
    );

    assert!(
        flat.contains("pubfntransaction<T>(")
            && flat.contains("implFnOnce(&mutTxHandle)->Result<T,TxError>"),
        "Tier 1 Database::transaction (FnOnce) is preserved"
    );

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
    assert!(
        flat.contains("buffer:Vec<("),
        "ConcurrentTxHandle has a private buffer field"
    );
    assert!(
        flat.contains("__id_bytes"),
        "write-set uses logical id bytes"
    );
    assert!(
        !flat.contains("(*__rowasu64).to_le_bytes()"),
        "write-set must not use physical row index to_le_bytes"
    );
    assert!(
        flat.contains("pubfnshared(self)->SharedDatabase"),
        "Database::shared() is generated"
    );
    assert!(
        flat.contains("__apply_and_commit_concurrent_buffer"),
        "Database::__apply_and_commit_concurrent_buffer is generated"
    );
}

#[test]
fn test_rust_generation_compaction_respects_live_snapshot() {
    let src = r#"
User {
  id: +uuid
  email: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let code = RustGenerator::generate(&schema).unwrap().code;

    assert!(
        code.contains("if self.in_transaction {") && code.contains("self.compact_deferred = true;"),
        "storage compact() defers while in_transaction is set"
    );

    assert!(
        code.contains("oldest_live_snapshot"),
        "generated code references oldest_live_snapshot for the keep-set bound"
    );
}

#[test]
fn test_rust_generation_coordinated_client() {
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

    assert!(
        code.contains("pub struct CoordinatedDatabase"),
        "CoordinatedDatabase struct emitted"
    );

    assert!(
        code.contains("pub fn connect("),
        "Database::connect emitted"
    );
    assert!(
        !code.contains("connect_coordinator"),
        "old lock-taking connect_coordinator removed (#84 lock-skip fix)"
    );

    assert!(
        code.contains("pub fn transaction_coordinated"),
        "CoordinatedDatabase::transaction_coordinated emitted"
    );

    assert!(
        code.contains("__apply_and_commit_concurrent_buffer"),
        "coordinator path reuses __apply_and_commit_concurrent_buffer (no drift)"
    );

    assert!(
        code.contains("as_bytes().to_vec()"),
        "model tags forwarded as opaque bytes, not decoded fields"
    );

    assert!(
        code.contains("pub struct SharedDatabase"),
        "SharedDatabase still present (additive, no breakage)"
    );
    assert!(
        code.contains("pub fn transaction_concurrent"),
        "transaction_concurrent still present (additive, no breakage)"
    );

    assert!(
        code.contains("forgedb_coordinator"),
        "generated code references the forgedb-coordinator substrate crate"
    );

    assert!(
        !code.contains("__row_indices.push(0)"),
        "row_indices must NOT be placeholder 0 — real positions from __apply_and_commit (T3-3)"
    );
    assert!(
        code.contains("__peer_refresh"),
        "peer read-currency: __peer_refresh method emitted (T3-8)"
    );

    assert!(
        code.contains("__sync_columns_from_disk"),
        "peer refresh syncs ALL columns from disk (not tombstone-only) — #84"
    );
    assert!(
        code.contains("sync_from_disk"),
        "peer refresh reads shared column live length via sync_from_disk (T3-8)"
    );
    assert!(
        code.contains("let __from = self.user.row_count;")
            && code.contains("self.user.__reindex_delta(__from);"),
        "peer refresh folds only new rows via __reindex_delta (#161-B), not a full rebuild"
    );

    assert!(
        code.contains("last_refreshed_lsn"),
        "last_refreshed_lsn tracks peer refresh cursor (T3-8)"
    );

    assert!(
        code.contains("pub fn transaction_concurrent"),
        "Tier 2 transaction_concurrent unchanged (T3-5)"
    );

    assert!(
        code.contains("__open_with_lock(root, None)"),
        "G1: coordinated open (connect) is LOCK-FREE — __open_with_lock(root, None)"
    );
    assert!(
        code.contains("DirLock::acquire"),
        "G1: standalone open_at still self-acquires the #89 DirLock"
    );
    assert!(
        code.contains("CoordinatorUnavailable"),
        "G1: connect surfaces CoordinatorUnavailable (never a lock-free standalone writer)"
    );

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

    assert!(
        code.contains("coordinator: Committed ack error"),
        "the Committed ack arm keeps its diagnostic and does not become fatal"
    );
}

fn parse_forge(src: &str) -> Schema {
    let mut p = forgedb_parser::Parser::new(src).unwrap();
    p.parse().unwrap()
}

fn sample_transform_crate() -> (String, forgedb_codegen::TransformCrate) {
    let v1 = parse_forge("User {\n  id: +uuid\n  age: u32\n}\n");
    let v2 = parse_forge("User {\n  id: +uuid\n  age: u32\n  bio: string?\n}\n");
    let v3 = parse_forge("User {\n  id: +uuid\n  age: string\n  bio: string?\n}\n");
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
                    field_copies: vec![],
                    field_null_fills: vec![],
                }],
                authored_src: None,
                escape: None,
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
                escape: None,
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
    assert!(
        main.contains("v1::Database") && main.contains("v3::Database"),
        "replay reads/writes via the embedded per-version typed structs"
    );
}

#[test]
fn test_transform_bin_replay_is_straightline() {
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
    let schema = go_binding_schema();
    let code = GoGenerator::generate(&schema, SYM, FP).unwrap().code;
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(code.contains("package forgedb"), "emits a Go package");
    assert!(code.contains("import \"C\""), "binds the C ABI over cgo");
    assert!(
        !code.to_uppercase().contains("EXPERIMENTAL"),
        "no experimental marker after de-experimentalization (#204)"
    );
    assert!(code.contains("func Open(root string) (*DB, error)"), "the Open lifecycle entry");
    assert!(flat.contains("func(db*DB)Commit()error"), "Commit wraps the commit entry point");
    assert!(flat.contains(&format!("C.{SYM}open(")), "Open calls the prefixed open symbol");
    assert!(
        flat.contains(&format!("C.{SYM}free_buffer(")),
        "buffers freed via the prefixed free_buffer symbol"
    );
    assert!(!flat.contains("C.forgedb_"), "a constant-prefixed C call survived:\n{code}");
    assert!(
        !code.contains("%SYM%"),
        "an unsubstituted symbol placeholder reached the emitted Go"
    );

    for m in ["User", "Post", "Tag", "Reading"] {
        assert!(code.contains(&format!("type {m} struct {{")), "row struct for {m}");
    }
    assert!(code.contains("Id string `json:\"id,omitempty\"`"), "uuid id is an omitempty string");
    assert!(code.contains("Age *int32 `json:\"age\"`"), "nullable i32 maps to *int32");
    assert!(code.contains("Author string `json:\"author\"`"), "required FK maps to a string id");
    assert!(code.contains("Id uint64 `json:\"id,omitempty\"`") || code.contains("Id uint64 `json:\"id\"`"), "u64 PK id is uint64");

    for (name, snake) in [("User", "user"), ("Post", "post"), ("Reading", "reading")] {
        for op in ["Insert", "Get", "Count", "All", "Update", "Delete"] {
            assert!(code.contains(&format!("func (db *DB) {op}{name}(")), "method {op}{name}");
        }
        assert!(
            code.contains(&format!("C.{SYM}{snake}_insert(")),
            "insert calls the C symbol"
        );
        assert!(code.contains(&format!("func (db *DB) Get{name}At(")), "snapshot _at read for {name}");
    }

    assert!(
        code.contains("func (db *DB) PostAuthor(")
            && flat.contains(&format!("C.{SYM}post_author(")),
        "forward FK Post.author"
    );
    assert!(
        code.contains("func (db *DB) UserPosts(")
            && flat.contains(&format!("C.{SYM}user_posts(")),
        "reverse 1:M User.posts"
    );
    assert!(
        flat.contains(&format!("C.{SYM}link_post_tag("))
            && flat.contains(&format!("C.{SYM}unlink_post_tag(")),
        "M2M link/unlink for Post<->Tag"
    );
    assert!(code.contains("func (db *DB) Link"), "an M2M Link method is generated");

    assert!(code.contains("type Status string"), "enum generates a named Go type");
    assert!(code.contains("StatusDraft Status = \"Draft\""), "enum variant const");
    assert!(code.contains("Status Status `json:\"status\"`"), "enum-typed field uses the enum type");

    assert!(code.contains("type Result[T any] struct"), "generic async Result type");
    assert!(code.contains("func runAsync["), "the generic async driver");
    assert!(code.contains("C.forgedbGoRegister()"), "registers the completion callback");
    for (name, snake) in [("User", "user"), ("Post", "post"), ("Reading", "reading")] {
        for (op, sym) in [
            ("Insert", "insert"), ("Get", "get"), ("Count", "count"),
            ("All", "all"), ("Update", "update"), ("Delete", "delete"),
        ] {
            assert!(code.contains(&format!("func (db *DB) {op}{name}Async(")), "async {op}{name}");
            assert!(
                code.contains(&format!("C.{SYM}{snake}_{sym}_async(")),
                "async C symbol {snake}_{sym}"
            );
        }
    }

    assert!(code.contains("snap *Snapshot, id string) ([]"), "an M2M _at getter over a snapshot");

    assert!(GoGenerator::needs_arrow(&schema), "schema has arrow-exportable columns");
    let arrow = GoGenerator::generate_arrow(&schema, SYM).expect("arrow file emitted").code;
    assert!(arrow.contains("arrow-go/v18/arrow/cdata"), "imports arrow-go cdata");
    assert!(arrow.contains("cdata.ImportCArray("), "imports the FFI C-Data-Interface export");
    assert!(
        arrow.contains("func (db *DB) ExportReadingValueArrow() (arrow.Array, error)"),
        "a per-column Export<Model><Field>Arrow method (i64 value column)"
    );
    assert!(
        arrow.contains(&format!("C.{SYM}reading_value_export_arrow(")),
        "calls the FFI export symbol"
    );
    assert!(GoGenerator::go_mod_scaffold("forgedb", true).contains("arrow-go/v18"), "go.mod pins arrow-go when needed");
    assert!(!GoGenerator::go_mod_scaffold("forgedb", false).contains("arrow-go"), "no arrow dep when unneeded");

    for forbidden in ["forgedb_query", "switch model", "predicate", "QueryBuilder", "reflect."] {
        assert!(!code.contains(forbidden), "must not emit generic query surface: {forbidden}");
    }
}

fn go_c_symbols(go_code: &str) -> Vec<String> {
    let bytes = go_code.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(pos) = go_code[i..].find("C.") {
        let start = i + pos + 2;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        out.push(go_code[start..end].to_string());
        i = end.max(start);
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn test_go_calls_match_ffi_symbols() {
    let schema = go_binding_schema();
    let mut go_code = GoGenerator::generate(&schema, SYM, FP).unwrap().code;
    go_code.push_str(&GoGenerator::generate_async_bridge(SYM).code);
    if let Some(arrow) = GoGenerator::generate_arrow(&schema, SYM) {
        go_code.push_str(&arrow.code);
    }
    let header = FfiGenerator::generate_header(&schema, SYM, FP).unwrap().code;
    let ffi_flat: String = FfiGenerator::generate(&schema, SYM)
        .unwrap()
        .code
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let engine: Vec<String> = go_c_symbols(&go_code)
        .into_iter()
        .filter(|s| s.starts_with(SYM))
        .collect();
    assert!(
        engine.len() > 10,
        "expected the Go binding to reference many engine symbols, found {}: {engine:?}",
        engine.len()
    );

    for sym in &engine {
        assert!(
            ffi_flat.contains(&format!("fn{sym}(")),
            "Go calls C.{sym} but the FFI generator emits no such symbol (drift!)"
        );
        assert!(
            header.contains(&format!("{sym}(")),
            "Go calls C.{sym} but forgedb.h declares no such prototype (drift!)"
        );
    }

    assert!(
        go_code.contains(&format!("//export {SYM}GoCompletion")),
        "the //export'ed completion callback must carry the app prefix:\n{go_code}"
    );
    assert!(
        !go_code.contains("//export forgedbGoCompletion"),
        "the unprefixed //export survived — two Go packages would collide on it"
    );

    for (label, text) in [("go", &go_code), ("header", &header)] {
        assert!(
            !text.contains("%SYM%"),
            "{label}: an unsubstituted symbol placeholder survived"
        );
        assert!(
            !text.contains("forgedb_open") && !text.contains("forgedb_free_buffer"),
            "{label}: a constant-prefixed engine symbol survived"
        );
    }
}

fn exported_c_symbols(ffi_code: &str) -> std::collections::BTreeSet<String> {
    let flat: String = ffi_code.chars().filter(|c| !c.is_whitespace()).collect();
    let marker = "extern\"C\"fn";
    let mut out = std::collections::BTreeSet::new();
    let mut attrs = 0usize;
    for chunk in flat.split("no_mangle").skip(1) {
        attrs += 1;
        let pos = chunk
            .find(marker)
            .expect("a no_mangle attribute with no extern \"C\" fn after it");
        let rest = &chunk[pos + marker.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(!name.is_empty(), "a no_mangle definition with no name");
        out.insert(name);
    }
    assert_eq!(
        out.len(),
        attrs,
        "two no_mangle definitions share one symbol name INSIDE one crate"
    );
    out
}

fn symbol_stems(
    set: &std::collections::BTreeSet<String>,
    pfx: &str,
) -> std::collections::BTreeSet<String> {
    set.iter()
        .map(|s| {
            s.strip_prefix(pfx)
                .unwrap_or_else(|| panic!("`{s}` does not carry the prefix `{pfx}`"))
                .to_string()
        })
        .collect()
}

fn go_exported_symbols(go_code: &str) -> std::collections::BTreeSet<String> {
    go_code
        .lines()
        .filter_map(|l| l.trim().strip_prefix("//export "))
        .map(|s| s.trim().to_string())
        .collect()
}

fn colliding_post_schema() -> Schema {
    let src = r#"
Post {
  id: +uuid
  title: string
  author: *Writer
}

Writer {
  id: +uuid
  name: string
  posts: [Post]
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    parser.parse().unwrap()
}

#[test]
fn test_two_apps_export_disjoint_ffi_symbols() {
    let schema = colliding_post_schema();

    let a = FfiGenerator::generate(&schema, SYM).unwrap().code;
    let b = FfiGenerator::generate(&schema, SYM_B).unwrap().code;

    let sa = exported_c_symbols(&a);
    let sb = exported_c_symbols(&b);

    assert!(
        sa.len() > 30,
        "expected a large exported surface, found {}: {sa:?}",
        sa.len()
    );
    assert_eq!(sa.len(), sb.len(), "the two apps export the same COUNT of symbols");

    let shared: Vec<&String> = sa.intersection(&sb).collect();
    assert!(
        shared.is_empty(),
        "two apps in one project export {} identical C symbols; a single Go \
         binary importing both would fail to link: {shared:?}",
        shared.len()
    );

    assert_eq!(
        symbol_stems(&sa, SYM),
        symbol_stems(&sb, SYM_B),
        "the two apps' symbol STEMS must be identical — the prefix is the only difference"
    );

    let stems = symbol_stems(&sa, SYM);
    for stem in ["open", "close", "free_buffer", "post_insert", "post_author", "writer_posts"] {
        assert!(
            stems.contains(stem),
            "stem `{stem}` missing from the exported surface: {stems:?}"
        );
    }

    let go_a = {
        let mut s = GoGenerator::generate(&schema, SYM, FP).unwrap().code;
        s.push_str(&GoGenerator::generate_async_bridge(SYM).code);
        s
    };
    let go_b = {
        let mut s = GoGenerator::generate(&schema, SYM_B, FP).unwrap().code;
        s.push_str(&GoGenerator::generate_async_bridge(SYM_B).code);
        s
    };

    let ea = go_exported_symbols(&go_a);
    let eb = go_exported_symbols(&go_b);
    assert!(!ea.is_empty(), "the Go package //exports at least the completion callback");
    assert!(
        ea.is_disjoint(&eb),
        "two Go packages //export the same C symbol: {:?}",
        ea.intersection(&eb).collect::<Vec<_>>()
    );

    let union_a: std::collections::BTreeSet<String> = sa.union(&ea).cloned().collect();
    let union_b: std::collections::BTreeSet<String> = sb.union(&eb).cloned().collect();
    assert!(
        union_a.is_disjoint(&union_b),
        "the two packages' external C symbols overlap: {:?}",
        union_a.intersection(&union_b).collect::<Vec<_>>()
    );

    let calls_a: Vec<String> = go_c_symbols(&go_a)
        .into_iter()
        .filter(|s| s.starts_with(SYM) || s.starts_with(SYM_B))
        .collect();
    assert!(calls_a.len() > 10, "expected many engine calls, found {calls_a:?}");
    for sym in &calls_a {
        assert!(sa.contains(sym), "app A calls `{sym}`, which app A does not export");
        assert!(!sb.contains(sym), "app A calls into app B's engine: `{sym}`");
    }
}

#[test]
fn test_ffi_cache_package_manifest() {
    let manifest = FfiGenerator::cargo_toml("blog-3f2a1b4c5d6e7f80-ffi", CORE_PKG);

    assert!(
        manifest.contains(r#"crate-type = ["cdylib", "rlib", "staticlib"]"#),
        "the ffi package must emit a staticlib as well:\n{manifest}"
    );

    for pin in [
        "forgedb-storage", "forgedb-types", "forgedb-changefeed", "forgedb-wal",
        "forgedb-compaction", "forgedb-txn", "forgedb-coordinator",
        "forgedb-query-params",
    ] {
        assert!(
            !manifest.contains(pin),
            "the wrapper still pins `{pin}` instead of routing through core:\n{manifest}"
        );
    }
    assert!(
        manifest.contains(&format!(r#"forgedb_core = {{ package = "{CORE_PKG}", path = "../core" }}"#)),
        "the core dependency must be RENAMED so no generated source carries the hash:\n{manifest}"
    );

    assert!(
        !manifest.contains("[profile"),
        "a profile table in a member is silently ignored; it must not be here:\n{manifest}"
    );
    assert!(
        !manifest.contains("panic ="),
        "the unwind floor belongs on the driver's invocation, not in a member manifest:\n{manifest}"
    );

    let code = FfiGenerator::generate(&colliding_post_schema(), SYM).unwrap().code;
    assert!(
        code.contains("use forgedb_core as database"),
        "the source reaches the database through the fixed alias:\n{}",
        code.lines().take(40).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !code.contains(CORE_PKG),
        "the per-app core PACKAGE NAME leaked into generated source"
    );
}

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

    assert!(code.contains("pub enum Role {"), "enum type missing");
    assert!(code.contains("pub struct Account {"), "model struct missing");
    assert!(code.contains("pub struct AccountSummary {"), "projection struct missing");

    for m in ["get_account", "list_account", "create_account", "update_account", "delete_account"] {
        assert!(code.contains(&format!("pub async fn {m}(")), "missing method {m}");
    }
    assert!(code.contains("pub async fn get_account_summary("), "projection get missing");
    assert!(code.contains("pub async fn list_account_summary("), "projection list missing");

    assert!(code.contains("pub struct ForgeDbError"), "typed error missing");
    assert!(code.contains("ListResult<Account>"), "paginated list result missing");

    assert!(code.contains("pub owner: String"), "required FK should be String");
    assert!(code.contains("pub reviewer: Option<String>"), "optional FK should be Option<String>");
    assert!(code.contains("pub balance: String"), "decimal should be String");
    assert!(code.contains("pub tags: serde_json::Value"), "json should be opaque Value");
    assert!(code.contains("pub projects: serde_json::Value"), "virtual relation should be opaque Value");

    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

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

    for m in ["GetAccount", "ListAccount", "CreateAccount", "UpdateAccount", "DeleteAccount"] {
        assert!(code.contains(&format!("func (c *Client) {m}(")), "missing method {m}");
    }
    assert!(code.contains("func (c *Client) GetAccountSummary("), "projection get missing");
    assert!(code.contains("func (c *Client) ListAccountSummary("), "projection list missing");

    assert!(code.contains("type ForgeDbError struct"), "typed error missing");
    assert!(code.contains("ListResult[Account]"), "generic list result missing");

    assert!(code.contains("Owner string `json:\"owner\"`"), "required FK should be string");
    assert!(code.contains("Reviewer *string `json:\"reviewer\"`"), "optional FK should be *string");
    assert!(code.contains("Balance string `json:\"balance\"`"), "decimal should be string");
    assert!(code.contains("Tags json.RawMessage `json:\"tags\"`"), "json should be RawMessage");
    assert!(code.contains("Projects json.RawMessage `json:\"projects\"`"), "virtual relation opaque");

    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

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

    for m in ["get_account", "list_account", "create_account", "update_account", "delete_account"] {
        assert!(code.contains(&format!("def {m}(")), "missing method {m}");
    }
    assert!(code.contains("def get_account_summary("), "projection get missing");
    assert!(code.contains("def list_account_summary("), "projection list missing");

    assert!(code.contains("class ForgeDbError(Exception):"), "typed error missing");
    assert!(code.contains("class ListResult(Generic[T]):"), "list result missing");

    assert!(code.contains("owner: str"), "required FK should be str");
    assert!(code.contains("reviewer: Optional[str] = None"), "optional FK should be Optional[str]");
    assert!(code.contains("balance: str"), "decimal should be str");
    assert!(code.contains("tags: Any = None"), "json should be Any (defaulted)");
    assert!(code.contains("projects: Any = None"), "virtual relation should be Any (defaulted)");

    assert!(code.contains("/api/audit-event/"), "multi-word model should kebab-case");

    let create_idx = code.find("class AuditEventCreate:").expect("create dataclass present");
    let create_block: String = code[create_idx..].chars().take(200).collect();
    assert!(create_block.contains("kind: str"), "create keeps required kind");
    assert!(!create_block.contains("id:") && !create_block.contains("at:"),
        "create must omit +uuid/+timestamp autos, got:\n{create_block}");

    insta::assert_snapshot!(code);
}

#[test]
fn test_rust_generation_borrowed_scan_view() {
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

    assert!(
        flat.contains("pub struct UserScanRef<'a> {"),
        "UserScanRef is emitted.\nGot: {flat}"
    );
    assert!(
        flat.contains(
            "pub __slot: usize, pub id: Uuid, pub email: &'a str, \
             pub bio: Option<&'a str>, pub age: u32, pub score: f64, }"
        ),
        "UserScanRef borrows strings and mirrors every other scan field type.\nGot: {flat}"
    );
    let ref_at = flat.find("pub struct UserScanRef").expect("has UserScanRef");
    let ref_decl = &flat[ref_at.saturating_sub(120)..ref_at];
    assert!(
        !ref_decl.contains("Serialize") && !ref_decl.contains("ToSchema"),
        "UserScanRef must stay internal — no wire derives.\nGot: {ref_decl}"
    );

    assert!(
        flat.contains(".email_col .read_str(__slot)"),
        "buffered scan must borrow the string slot via read_str.\nGot: {flat}"
    );
    assert!(
        flat.contains("Some(&raw[1..])"),
        "nullable string borrows past the presence tag instead of copying"
    );

    assert!(
        flat.contains(".email_col .read_string(row_index)"),
        "the positional read path keeps the owned decode"
    );

    assert!(
        flat.contains(
            "pub fn __with_scan<R>( &self, sel: Option<Vec<usize>>, \
             keep: impl Fn(&UserScanRef<'_>) -> bool, \
             f: impl FnOnce(&mut Vec<UserScanRef<'_>>) -> R, ) -> R"
        ),
        "__with_scan is the scan scope: a selection, a predicate, and a callback.\nGot: {flat}"
    );
    assert!(
        flat.contains("if keep(&__row_ref) { __refs.push(__row_ref); }"),
        "survivors stay borrowed inside the scope.\nGot: {flat}"
    );
    assert!(
        flat.contains("f(&mut __refs) }"),
        "the scope returns only what the callback returns.\nGot: {flat}"
    );
}

#[test]
fn test_api_generation_borrowed_scan_filter() {
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

    assert!(code.contains("fn __user_scan_matches("), "scan filter emitted");
    assert!(!code.contains("__user_scan_matches_ref"), "the owned-operand twin is gone");
    assert!(
        flat.contains(
            "fn __user_scan_matches( record: &super::UserScanRef<'_>, \
             params: &HashMap<String, String>, ) -> bool"
        ),
        "the one scan filter takes the borrowed view.\nGot: {flat}"
    );

    assert!(
        flat.contains("__with_scan( None, |r| __keep_all || __user_scan_matches(r, &params),"),
        "the live-query scans filter on the borrowed view inside the scope.\nGot: {flat}"
    );
    assert!(
        flat.matches("|r| __keep_all || __user_scan_matches(r, &params)").count() == 3,
        "REST list + live-query init + live-query re-run, one scan each — and #288 hoists \
         at every one of them, so the count pins the hoist's coverage too.\nGot: {flat}"
    );
    assert!(
        !flat.contains(".retain(|r| __user_scan_matches(r, &params))")
            && !flat.contains(".retain(|r| __keep_all || __user_scan_matches(r, &params))"),
        "no post-scan retain over decoded rows remains (both spellings — #288 changed the \
         closure body, and a negative pinned to the old one alone would pass vacuously)"
    );

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

    assert!(
        !flat.contains("matchserde_json::to_value"),
        "no index key may route through a serde_json::Value match (#230).\nGot: {code}"
    );
    assert!(
        !flat.contains(r"String::from('\u{3}')"),
        "the unreachable serialization-error tag must no longer be emitted (#230)"
    );

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
    assert!(
        flat.contains(r#"write!(__k,"{}",__forgedb_f64_key(*__v))"#),
        "f64 keys must be the total-order encoding (#242)"
    );
    assert!(
        !flat.contains("serde_json::Number::from_f64"),
        "the from_f64 path is the #242 defect — it must be gone entirely"
    );
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
    assert!(
        flat.contains(r#"__k.push('[');for(__i,__b)in__v.iter().enumerate()"#),
        "bytes(N) keys render the JSON array form"
    );

    assert!(
        flat.contains(r"Some(__v)=>{letmut__k=String::with_capacity(1+__v.len());"),
        "nullable string keys match the Option and reuse the non-nullable body"
    );
    assert!(
        flat.contains(r"None=>String::from('\u{0}'),"),
        "the None arm keys into the null bucket"
    );
    assert!(
        flat.contains(r"match&(record.editor){Some(__v)=>{letmut__buf=[0u8;36];"),
        "an optional FK keys through the Option arm, not as a bare Uuid (#230)"
    );
}

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

    assert!(
        flat.contains("if__v.chars().count()<3i64asusize"),
        "`@length(min: 3)` emits a floor check.\nGot: {code}"
    );
    assert!(
        !flat.contains(r#""floor",message:"lengthmustbe<=3""#),
        "`@length(min: 3)` must NOT be read as a maximum"
    );

    assert!(
        flat.contains("if__v.chars().count()>20i64asusize"),
        "`@length(max: 20)` emits a ceiling check"
    );

    assert!(
        flat.contains("__len<3i64asusize||__len>64i64asusize"),
        "`@length(min: 3, max: 64)` emits a range check"
    );
    assert!(
        flat.contains("__len<3i64asusize||__len>5i64asusize"),
        "`@length(3, 5)` is unchanged — still min, max"
    );

    assert!(
        flat.contains("if__v.chars().count()!=7i64asusize"),
        "`@length(7)` now emits an EQUALITY check (#235)"
    );
    assert!(
        !flat.contains("if__v.chars().count()>7i64asusize"),
        "`@length(7)` must no longer emit the old `> 7` maximum check"
    );

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

    for (field, path) in [
        ("plain", "__forgedb_big_bytes"),
        ("fingerprint", "__forgedb_big_bytes"),
        ("past", "__forgedb_big_bytes"),
        ("opt_hash", "__forgedb_big_bytes::option"),
        ("arr_big", "__forgedb_big_bytes::array"),
        ("many", "__forgedb_big_array"),
        ("digest", "__forgedb_big_bytes"),
        ("wide", "__forgedb_big_array"),
    ] {
        let decl = format!(r#"#[serde(with="{path}")]pub{field}:"#);
        assert!(
            flat.contains(&decl),
            "`{field}` is past serde's array ceiling and must use `{path}`"
        );
    }
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

    assert!(
        !code.contains("__forgedb_big_bytes") && !code.contains("__forgedb_big_array"),
        "a schema that stays within serde's array ceiling must carry no helper.\nGot: {code}"
    );
}

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

    assert!(
        !code.contains("__forgedb_f64_key"),
        "an unindexed f64 needs no key helper.\nGot: {code}"
    );
}

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

    assert!(
        code.contains("fn __forgedb_f64_key"),
        "an f64 reachable only as a composite component still needs the helper.\nGot: {code}"
    );
}

#[test]
fn test_generation_char_and_bytes_are_byte_identical() {
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

    let link = extract_struct(&code, "pub struct LinkScanRef<'a>");
    assert!(
        link.contains("PhantomData<&'a ()>"),
        "a scan view with no borrowing field must anchor 'a, else E0392:\n{link}"
    );

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

fn extract_struct<'a>(code: &'a str, decl: &str) -> &'a str {
    let start = code
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found in generated code"));
    let rest = &code[start..];
    let end = rest.find('}').expect("unterminated struct") + 1;
    &rest[..end]
}

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
    assert!(
        mentions_ident(&code, "number_index"),
        "`&` still builds its index (#258 must not regress under #187)"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("ValidationError::Unique{model:\"Invoice\",field:\"number\""),
        "`&` still enforces uniqueness (#258 must not regress under #187)"
    );
    assert!(
        !mentions_ident(&code, "__autoseq_id"),
        "a `+uuid` identity allocates from randomness, not a counter (#187)"
    );
}

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
    let storage = extract_struct(&code, "pub struct TicketStorage");
    assert!(
        storage.contains("__autoseq_id"),
        "the writer is where the counter lives (#187):\n{storage}"
    );
}

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
    assert!(
        compact[..destroy].contains("return;"),
        "a floor that cannot be persisted must ABORT the compaction, not proceed \
         (#187):\n{compact}"
    );
}

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

#[test]
fn test_rust_generation_conflict_visible_autos_emit_no_sequence_claim() {
    for src in [
        "Ticket {\n  id: +u64\n  title: string\n}\n",
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
}

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
    assert!(
        mentions_ident(&code, "__autoseq_id"),
        "a model with an integer auto still allocates alongside the junction (#187)"
    );
}

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

fn db_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    RustGenerator::generate(&schema).unwrap().code
}

fn column_init(code: &str, field: &str) -> String {
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let needle = format!("{field}_col: FixedColumn::new(");
    let at = flat.find(&needle).unwrap_or_else(|| {
        panic!("no FixedColumn init for `{field}` — it did not get a fixed column at all")
    });
    let tail = &flat[at..];
    tail[..tail.find(") .expect").unwrap_or(tail.len().min(200))].to_string()
}

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

#[test]
fn test_rust_generation_string_n_slot_widths() {
    for (decl, field, want) in [
        ("string(32)", "sku", 33usize),
        ("string(255)", "wide", 256),
        ("string(26!)", "key", 26),
        ("string(63) @utf8", "just_under", 63 * 4 + 1),
        ("string(64) @utf8", "just_over", 64 * 4 + 2),
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

#[test]
fn test_rust_generation_string_n_nullable_adds_a_presence_byte() {
    let code = db_for("Doc {\n  id: +uuid\n  note: string(10)?\n}\n");
    let init = column_init(&code, "note");
    assert!(
        init.contains("12usize"),
        "`string(10)?` is 1 (present) + 10 (payload) + 1 (prefix) = 12: {init}"
    );
}

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
    assert!(
        flat.contains("body_col: VariableColumn"),
        "bare `string` still gets a VariableColumn"
    );
    assert!(
        !flat.contains("self.sku_col.append_string"),
        "`string(32)` must not be appended through the variable-string codec"
    );
}

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

#[test]
fn test_rust_generation_string_n_scan_borrows_the_slot() {
    let code = db_for("Doc {\n  id: +uuid\n  key: string(26!)\n  sku: string(32)\n}\n");
    let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        flat.contains("key_col .read_str"),
        "`string(26!)` reads the whole slot as UTF-8: {flat}"
    );
    assert!(
        flat.contains("sku_col .read_slice"),
        "`string(32)` borrows the slot rather than copying it"
    );

    let scan_src = RustSource::generated("database.rs", code.clone());
    let scan = scan_src
        .method_named("__with_scan")
        .expect("the scan scope is emitted");
    assert_eq!(
        scan.call_count("read_bytes"),
        0,
        "a per-row `Vec` on the scan path is the one outcome #238 exists to avoid"
    );
}

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
    assert!(flat.contains("@utf8"), "the ascii diagnostic names the opt-in");
}

#[test]
fn test_rust_generation_utf8_drops_the_ascii_check() {
    let code = db_for("Doc {\n  id: +uuid\n  title: string(8) @utf8\n}\n");
    assert!(
        !code.contains("\"ascii\""),
        "@utf8 opts out of the one-byte-per-character alphabet"
    );
}

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
        ("go", GoGenerator::generate(&schema, SYM, FP).unwrap().code),
    ] {
        assert!(
            !code.contains("StringN") && !code.contains("string_n"),
            "{label} leaked the storage spelling onto the wire"
        );
    }
}

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

#[test]
fn test_api_generation_openapi_fk_follows_the_target_key() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();
    let doc = OpenApiGenerator::generate(&schema).unwrap().code;
    let spec: serde_json::Value = serde_json::from_str(&doc).unwrap();

    let post = &spec["components"]["schemas"]["Comment"]["properties"]["post"];
    assert_eq!(post["type"], "integer", "an FK to a u64 key is an integer: {post}");
    assert_eq!(post["format"], "int64", "{post}");

    let reply = &spec["components"]["schemas"]["Comment"]["properties"]["reply_to"];
    assert_eq!(reply["format"], "uuid", "a uuid-keyed FK target is untouched: {reply}");
}

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

#[test]
fn test_bindings_fk_type_equals_the_targets_own_id_type() {
    let mut parser = forgedb_parser::Parser::new(U64_PARENT_SRC).unwrap();
    let schema = parser.parse().unwrap();

    let go = GoGenerator::generate(&schema, SYM, FP).unwrap().code;
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

    assert!(
        code.contains(r#"__rescale(__row.taken_at, "Reading", "taken_at")"#),
        "a bare timestamp is rescaled: {code}"
    );
    assert!(
        code.contains("if let Some(__ts_opt) = &mut __row.maybe_at"),
        "a NULLABLE timestamp is reached (the shape the schema-blind pass misses): {code}"
    );
    assert!(
        code.contains("for __ts_elem in __row.marks.iter_mut()"),
        "every array element is rescaled: {code}"
    );
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains(r#""Reading","window.opened_at""#)
            && flat.contains(r#""Reading","window.closed_at""#),
        "struct-nested timestamps are rescaled and named by path: {code}"
    );
    assert!(
        !code.contains("__row.label ="),
        "a string field is not touched: {code}"
    );
    assert!(
        code.contains("checked_mul(1000000)"),
        "the multiply is checked: {code}"
    );
    assert!(
        code.contains("mod e1;") && code.contains("mod e2;"),
        "both engine generations are embedded: {code}"
    );
    assert!(
        code.contains("e1::Database::open_at") && code.contains("e2::Database::open_at"),
        "the reader half is e1 and the writer half is e2: {code}"
    );
}

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

#[test]
fn test_rust_generation_open_guard_has_two_distinct_arms() {
    let schema = engine_hop_schema();
    let code = RustGenerator::generate(&schema).unwrap().code;
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

fn api_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    ApiGenerator::generate(&schema).unwrap().code
}

fn wasm_for(src: &str) -> String {
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    WasmGenerator::generate(&schema).unwrap().code
}

fn flat(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
    let atmost = flat(&db_for("Doc {\n  id: string(26)\n  title: string\n}\n"));
    assert!(
        atmost.contains("InlineStr<26usize>"),
        "`string(26)` keys the same width as `string(26!)`"
    );
}

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

#[test]
fn test_api_generation_string_key_path_param_is_the_key_type() {
    let code = api_for("Sku {\n  id: string(26!)\n  title: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("id.parse::<InlineStr<26usize>>()"),
        "the path segment parses into the key type: {f:.800}"
    );
    assert!(
        !f.contains("id.parse::<Uuid>()"),
        "and never falls through to Uuid for a string-keyed model"
    );
}

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
    let at = f
        .find(r#"rule: "identity_alphabet""#)
        .expect("the alphabet rule is emitted");
    let window = &f[at..(at + 400).min(f.len())];
    assert!(
        window.contains("__c") && window.contains("__i"),
        "the message names the character and its byte offset: {window}"
    );
}

#[test]
fn test_rust_generation_string_key_alphabet_is_pchar_minus_percent() {
    let code = db_for("Doc {\n  id: string(64)\n  title: string\n}\n");
    let f = flat(&code);
    let at = f
        .find("__forgedb_identity_char_ok")
        .expect("the alphabet predicate is emitted as a named helper");
    let window = &f[at..(at + 900).min(f.len())];
    for admitted in ["'@'", "':'", "'!'", "'$'", "'&'", r"'\''", "'('", "')'", "'*'", "'+'",
                     "','", "';'", "'='", "'-'", "'.'", "'_'", "'~'"] {
        assert!(
            window.contains(admitted),
            "{admitted} is a pchar and must be admitted: {window}"
        );
    }
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

#[test]
fn test_rust_generation_string_keyed_target_keeps_the_relation_surface() {
    let code = db_for(
        "Airport {\n  id: string(3!)\n  city: string\n  flights: [Flight]\n}\n\n\
         Flight {\n  id: +uuid\n  origin: *Airport\n  alt: ?Airport\n}\n",
    );
    let f = flat(&code);

    assert!(
        f.contains("pub origin: InlineStr<3usize>"),
        "a required FK to a string-keyed parent is the parent's key: {f:.600}"
    );
    assert!(
        f.contains("pub alt: Option<InlineStr<3usize>>"),
        "and an optional FK is the same key, wrapped"
    );
    assert!(
        flat(&column_init(&code, "origin")).contains("3usize"),
        "the FK column is the identity column's width"
    );

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
    assert!(
        f.contains("FixedColumn::new") && f.contains("12usize"),
        "the string endpoint's junction column is 12 bytes wide"
    );
    assert!(
        f.contains("HashMap<Uuid, Vec<InlineStr<12usize>>>")
            || f.contains("HashMap<InlineStr<12usize>, Vec<Uuid>>"),
        "the in-memory traversal index keys on the endpoint key types: {f:.600}"
    );
}

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

#[test]
fn test_rust_generation_a_string_fk_is_an_inline_string_column() {
    let code = db_for(
        "Airport {\n  id: string(3!)\n  city: string\n  flights: [Flight]\n}\n\n\
         Flight {\n  id: +uuid\n  origin: *Airport\n  alt: ?Airport\n}\n",
    );
    let f = flat(&code);

    assert!(
        !f.contains("append_inline_string") && !f.contains("read_inline_string"),
        "no column method is named after the inline-string label: {f:.400}"
    );

    assert!(
        f.contains("let mut __buf = [0u8; 4usize]"),
        "a nullable string FK packs a tagged slot, not a transmuted Option: {f:.400}"
    );

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

    assert!(
        f.contains("self.flight.find_by_origin(&id)"),
        "the cascade borrows the key for the probe: {f:.400}"
    );
    assert!(
        f.contains("self.flight.find_by_alt(Some(&id))"),
        "and so does the optional one"
    );
}

#[test]
fn test_rust_generation_the_scan_view_holds_a_string_key_by_value() {
    let code = db_for("Airport {\n  id: string(3!)\n  city: string\n}\n");
    let f = flat(&code);
    assert!(
        f.contains("pub __slot: usize, pub id: InlineStr<3usize>"),
        "the scan view's key is owned and Copy: {f:.400}"
    );
    assert!(
        f.contains("pub city: &'a str"),
        "while an ordinary string column still borrows"
    );

    let bare = flat(&db_for("Tag {\n  id: string(8!)\n  weight: u32\n}\n"));
    assert!(
        bare.contains("PhantomData<&'a ()>"),
        "a key-only string model re-anchors 'a: {bare:.400}"
    );
}

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

    let other = db_for("Event {\n  id: u32\n  seq: +u64\n  note: string\n}\n");
    assert_eq!(
        flat(&other).contains("pub fn get(&self, id: u32)"),
        true,
        "precedence, not position"
    );
}

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

const PAGE_REF_SRC: &str = r#"
enum Tier { Free, Pro, Enterprise }

struct Dims {
  w: u32
  h: u32
}

Widget {
  id: +uuid
  label: string?
  price: decimal
  tier: ^Tier
  made_at: timestamp(ms)
  checksum: bytes(4)
  scores: [i32; 3]
  dims: Dims
  owner: *Maker
  parts: [Part]
  payload: json
  serial: ^u32
}

Maker {
  id: +uuid
  name: string
  widgets: [Widget]
}

Part {
  id: +uuid
  name: string
  widget: *Widget
}

Note {
  body: string
  id: +uuid
  weight: ^u32
}
"#;

#[test]
fn test_rust_generation_page_ref_field_order() {
    let f = flat(&db_for(PAGE_REF_SRC));

    assert!(
        f.contains(
            "pub struct WidgetScanRef<'a> { \
             pub __slot: usize, pub id: Uuid, pub label: Option<&'a str>, \
             pub price: rust_decimal::Decimal, pub tier: Tier, pub made_at: Timestamp, \
             pub checksum: [u8; 4usize], pub serial: u32, }"
        ),
        "the scan view is identity-first, filterable-only: {f}"
    );
    assert!(
        f.contains(
            "pub struct WidgetPageRef<'a> { pub id: Uuid, pub label: Option<&'a str>, \
             #[serde(with = \"rust_decimal::serde::str\")] pub price: \
             rust_decimal::Decimal, pub tier: Tier, pub made_at: Timestamp, pub \
             checksum: [u8; 4usize], pub scores: [i32; 3usize], pub dims: Dims, pub \
             owner: Uuid, pub parts: (), pub payload: serde_json::Value, pub serial: \
             u32, }"
        ),
        "the page view is model declaration order, every field: {f}"
    );
    assert!(
        f.contains(
            "pub struct Widget { #[serde(default)] pub id: Uuid, pub label: \
             Option<String>, #[schema(value_type = String)] #[serde(with = \
             \"rust_decimal::serde::str\")] pub price: rust_decimal::Decimal, pub \
             tier: Tier, #[schema(value_type = String)] pub made_at: Timestamp, pub \
             checksum: [u8; 4usize], pub scores: [i32; 3usize], pub dims: Dims, pub \
             owner: Uuid, pub parts: (), pub payload: serde_json::Value, pub serial: \
             u32, }"
        ),
        "and the model's order is what it is being held to: {f}"
    );

    assert!(
        f.contains("pub struct NotePageRef<'a> { pub body: &'a str, pub id: Uuid, pub weight: u32, }"),
        "an identity declared second stays second on the wire: {f}"
    );
    assert!(
        f.contains("pub id: Uuid, pub body: &'a str, pub weight: u32, }"),
        "while the scan view for the same model IS identity-first: {f}"
    );

    assert!(
        f.contains("#[derive(Debug, Clone)] pub struct WidgetScanRef<'a>"),
        "the scan view still derives neither Serialize nor ToSchema: {f}"
    );
    assert!(
        f.contains("#[derive(serde::Serialize)] pub struct WidgetPageRef<'a>"),
        "and the page view derives Serialize only — no Deserialize, no ToSchema: {f}"
    );
}

#[test]
fn test_rust_generation_page_ref_excludes_json_from_buffers() {
    let f = flat(&db_for(PAGE_REF_SRC));

    assert!(
        f.contains("pub payload: serde_json::Value,"),
        "json is an owned Value on the page view: {f}"
    );
    assert!(
        !f.contains("pub payload: &'a str") && !f.contains("pub payload: Option<&'a str>"),
        "and never a borrowed passthrough: {f}"
    );

    assert!(
        f.contains(
            "payload_col: self .payload_col .gather_buffered(&__page_rows) \
             .expect(\"Failed to bulk-load page column\")"
        ),
        "json is gathered over the page's rows: {f}"
    );
    assert!(
        !f.contains(
            "payload_col: self .payload_col .gather_buffered(&__rows) \
             .expect(\"Failed to bulk-load scan column\")"
        ),
        "json never enters the full-table scan gather: {f}"
    );

    assert!(
        f.contains(
            "let payload_value = { let raw = __page_bufs .payload_col .read_string(__pslot) \
             .expect(\"Failed to read string\"); serde_json::from_str(&raw)\
             .expect(\"Failed to deserialize json\") };"
        ),
        "the page decodes json through read_string + from_str: {f}"
    );
    assert!(
        f.contains(
            "let payload_value = { let raw = self .payload_col .read_string(row_index) \
             .expect(\"Failed to read string\"); serde_json::from_str(&raw)\
             .expect(\"Failed to deserialize json\") };"
        ),
        "which is exactly what read_at does: {f}"
    );
    assert!(
        !f.contains("payload_col .read_str(") && !f.contains("payload_col.read_str("),
        "the borrowed accessor is never used for json: {f}"
    );

    assert!(
        f.contains(
            "struct __WidgetPageBufs { scores_col: forgedb_storage::BufferedFixedColumn, \
             dims_col: forgedb_storage::BufferedFixedColumn, \
             owner_col: forgedb_storage::BufferedFixedColumn, \
             payload_col: forgedb_storage::BufferedVariableColumn, } let __page_bufs"
        ),
        "the page gather is exactly the scan set's complement: {f}"
    );
    assert!(
        f.contains("parts: (),"),
        "a virtual relation is defaulted, not gathered: {f}"
    );
    assert!(
        f.contains("struct __MakerPageBufs {} let __page_bufs = __MakerPageBufs {};"),
        "and an all-scan model's page gather is empty: {f}"
    );
}

#[test]
fn test_rust_generation_page_rows_map_through_the_recorded_slot() {
    let f = flat(&db_for(PAGE_REF_SRC));

    assert!(
        f.contains("let __page_rows: Vec<usize> = __page .iter() .map(|__r| __rows[__r.__slot]) .collect();"),
        "the page's physical rows come from the ref's recorded slot: {f}"
    );
    let method_body = |name: &str| -> &str {
        let start = f
            .find(name)
            .unwrap_or_else(|| panic!("{name} is emitted: {f}"));
        let end = f[start + 1..]
            .find("pub fn ")
            .map(|i| start + 1 + i)
            .unwrap_or(f.len());
        &f[start..end]
    };
    assert!(
        !method_body("pub fn __with_page").contains("__rows[__start"),
        "not a slice of the selection: {f}"
    );
    assert!(
        method_body("pub fn __with_fast_page")
            .contains("gather_buffered(&__rows[__start..__end])"),
        "#281: the fast page gathers the page's rows, not the table's: {f}"
    );
    assert!(
        f.contains("let __row_ref = WidgetScanRef { __slot,"),
        "and the slot is recorded at decode time, before any reordering: {f}"
    );

    assert!(
        f.contains(
            "let __total = __refs.len(); sort(&mut __refs); let __start = offset.min(__total); \
             let __end = offset.saturating_add(limit).min(__total); \
             let __page = &__refs[__start..__end];"
        ),
        "total is counted before the page is sliced, and the slice clamps both ends: {f}"
    );
}

#[test]
fn test_api_generation_projection_model_keeps_an_owned_page() {
    let src = r#"
Post {
  @projection(card: title, views)
  id: +uuid
  title: string
  body: string
  views: u32
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let api = ApiGenerator::generate(&schema).unwrap().code;
    let f: String = api.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        f.contains("if params.get(\"projection\").is_none() {"),
        "#226: the borrowed page is guarded on there being no ?projection=: {f}"
    );
    assert!(
        f.contains("return db .post .__with_page("),
        "#226: a projected model still takes the page scope when unprojected: {f}"
    );
    assert!(
        f.contains("db .post .__with_scan("),
        "#226: ?projection= keeps the #160/#228 owned narrow path: {f}"
    );
    assert!(
        f.contains(".filter_map(|__id| db.post.get(*__id))"),
        "#226: the owned path still full-materializes only the page: {f}"
    );
    assert!(
        f.contains("let __data: Vec<super::PostCard> = page .iter() .map(|r| super::PostCard {"),
        "the projection arm field-copies the OWNED page — the reason it survives: {f}"
    );
    assert_eq!(
        f.matches("let __sel: Option<Vec<usize>> =").count(),
        2,
        "#226: each branch computes its own moved selection: {f}"
    );
}

#[test]
fn test_api_generation_unfiltered_list_hoists_the_predicate() {
    let src = r#"
Post {
  id: +uuid
  title: string
  views: ^u64
  published: bool
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let api = ApiGenerator::generate(&schema).unwrap().code;
    let f = flat(&api);

    assert!(
        f.contains("fn __post_is_unfiltered(params: &HashMap<String, String>) -> bool"),
        "#288: the per-model unfiltered predicate is emitted: {f}"
    );
    for field in ["id", "title", "views", "published"] {
        assert!(
            f.contains(&format!("if params.contains_key(\"{field}\") {{ return false; }}")),
            "#288: the predicate derives `{field}` from the same iteration as the \
             per-field checks, so the two cannot disagree about what is filterable: {f}"
        );
    }
    assert!(
        !f.contains("__post_is_unfiltered") || !f.contains("params.contains_key(\"as_of\")"),
        "#288: the predicate must not carry a reserved-name exclusion list: {f}"
    );

    assert!(
        f.contains("let __keep_all: bool = __post_is_unfiltered(&params);"),
        "#288: the predicate is hoisted to a binding, not called per row: {f}"
    );
    assert!(
        f.contains("|r| __keep_all || __post_scan_matches(r, &params)"),
        "#288: the per-row closure short-circuits on the hoisted bool: {f}"
    );

    let sel = f
        .find("__rows_by_views(")
        .expect("the index-pushdown selection expression");
    assert_eq!(
        f.matches("__rows_by_views(").count(),
        1,
        "#281: one pushdown site on the list path, so `sel` below is unambiguous: {f}"
    );
    let keep = f
        .find("let __keep_all: bool =")
        .expect("hoisted predicate binding");
    let fast = f
        .find("if __keep_all && qp.sort.is_none() {")
        .expect("#281: fast-page branch");
    let page = f.find("return db .post .__with_page(").expect("page call");
    assert!(
        keep < fast && fast < sel && sel < page,
        "#288/#281: the predicate is answered first, gates the fast page, and only \
         then does the scan path probe the index \
         (keep={keep}, fast={fast}, sel={sel}, page={page}): {f}"
    );
}

#[test]
fn test_api_generation_reserved_name_field_is_filterable() {
    let src = r#"
Gauge {
  id: +uuid
  limit: u32
  offset: u32
  sort: string
  order: string
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let api = ApiGenerator::generate(&schema).unwrap().code;
    let f = flat(&api);

    for field in ["limit", "offset", "sort", "order"] {
        assert!(
            f.contains(&format!("if params.contains_key(\"{field}\") {{ return false; }}")),
            "#288: `{field}` is a declared filterable field here, so naming it in the \
             query string must defeat the fast test: {f}"
        );
    }
}

#[test]
fn test_api_generation_zero_filterable_model_has_no_unused_param() {
    let src = r#"
Doc {
  id: +uuid
  title: string
}

Tag {
  id: +uuid
  name: string
}

Link {
  id: *Doc
  other: *Tag
  meta: json
}
"#;
    let mut parser = forgedb_parser::Parser::new(src).unwrap();
    let schema = parser.parse().unwrap();
    let api = ApiGenerator::generate(&schema).unwrap().code;
    let f = flat(&api);

    assert!(
        f.contains("fn __link_is_unfiltered(_params: &HashMap<String, String>) -> bool"),
        "#288: a zero-filterable model names the parameter `_params`: {f}"
    );
    assert!(
        f.contains("fn __doc_is_unfiltered(params: &HashMap<String, String>) -> bool"),
        "#288: a model with filterable fields keeps the live parameter: {f}"
    );
}

#[test]
fn test_rust_generation_web_off_emits_no_utoipa() {
    let mut parser = forgedb_parser::Parser::new(
        r#"
enum Status { Active, Archived }

struct Point {
  x: f64
  y: f64
}

Note {
  id: +uuid
  body: string
  status: Status
  at: timestamp
  origin: Point
  tags: [string; 3]
}
"#,
    )
    .unwrap();
    let schema = parser.parse().unwrap();

    let cfg = GenConfig {
        web: false,
        ..GenConfig::DEFAULT
    };
    let off = RustGenerator::generate_with_config(&schema, 1, cfg)
        .unwrap()
        .code;

    let derives_to_schema = |code: &str| {
        code.lines()
            .any(|l| l.contains("#[derive(") && l.contains("ToSchema"))
    };

    assert!(
        !off.contains("use utoipa::"),
        "the utoipa import survived:\n{off}"
    );
    assert!(!derives_to_schema(&off), "a ToSchema derive survived");
    assert!(
        !off.contains("#[schema("),
        "a #[schema(..)] attribute survived without its derive"
    );

    let on = RustGenerator::generate_with_config(&schema, 1, GenConfig::DEFAULT)
        .unwrap()
        .code;
    assert!(on.contains("use utoipa::ToSchema"), "the ON path lost its import");
    assert!(derives_to_schema(&on), "the ON path lost its derive");
    assert!(
        on.contains("#[schema("),
        "this schema no longer exercises #[schema(..)], so the OFF assertion above is vacuous"
    );
}

fn answered_transform_crate() -> (String, forgedb_codegen::TransformCrate) {
    let v1 = parse_forge("Post {\n  id: +uuid\n  title: string\n  views: u32\n}\n");
    let v2 = parse_forge(
        "Post {\n  id: +uuid\n  title: string\n  views: string\n  slug: string\n  \
         summary: string\n}\n",
    );
    let v1: &'static Schema = Box::leak(Box::new(v1));
    let v2: &'static Schema = Box::leak(Box::new(v2));

    let plan = TransformPlan {
        versions: vec![
            VersionSchema { version: 1, schema: v1 },
            VersionSchema { version: 2, schema: v2 },
        ],
        hops: vec![HopPlan {
            from_version: 1,
            to_version: 2,
            migration_id: "m1".to_string(),
            model_ops: vec![ModelOp {
                model: "Post".to_string(),
                source_model: "Post".to_string(),
                field_renames: vec![],
                field_removes: vec![],
                field_adds: vec![("slug".to_string(), "\"untitled\"".to_string())],
                field_copies: vec![("title".to_string(), "summary".to_string())],
                field_null_fills: vec![],
            }],
            authored_src: None,
            escape: Some(forgedb_codegen::EscapeBridge {
                program: "/usr/bin/bun".to_string(),
                args: vec!["/cache/escape/m1/transform.ts".to_string()],
            }),
        }],
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

fn code_only(main: &str) -> String {
    main.lines()
        .map(|l| {
            let l = match l.find("///") {
                Some(i) => &l[..i],
                None => l,
            };
            match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_transform_generation_answers_are_lowered() {
    let (main, _) = answered_transform_crate();
    let code = code_only(&main);
    let dense: String = code.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        dense.contains(r#"serde_json::from_str("\"untitled\"").unwrap()"#),
        "a Constant is baked as a JSON literal at its own call site:\n{code}"
    );
    assert!(
        dense.contains(r#"__obj.get("title")"#) && dense.contains(r#"__obj.insert("summary""#),
        "a CopyField is a per-row read of one named field into another:\n{code}"
    );
    assert!(
        dense.contains(r#"__escape.row("Post",__j)"#),
        "an Escape is a call with the MODEL NAME as a literal at the call site — \
         the same shape `authored_transform` already has:\n{code}"
    );

    for forbidden in [
        "Answer",
        "CopyField",
        "Constant {",
        "scaffold_checksum",
        "EscapeLanguage",
        "hop_body_class",
    ] {
        assert!(
            !code.contains(forbidden),
            "`{forbidden}` reached the emitted crate. The answer is a COMPILE-TIME \
             input that is lowered; carrying it as data and matching on it is the \
             inversion this guard exists for:\n{code}"
        );
    }

    for forbidden in [
        "Vec<Step",
        "for __model in",
        "for model in",
        "&[\"Post\"",
        "MODELS",
        "descriptor",
    ] {
        assert!(
            !code.contains(forbidden),
            "the bridge must never be given a model list or an op table (found \
             {forbidden:?}):\n{code}"
        );
    }
}

#[test]
fn test_transform_escape_bridge_adds_no_dependency() {
    let (main, crate_out) = answered_transform_crate();
    let toml = &crate_out.cargo_toml;
    assert!(
        !toml.contains("forgedb-migrations =") && !toml.contains("forgedb-parser ="),
        "the escape bridge must not drag in the parser or the migration crate"
    );
    assert!(
        main.contains("std::process::Command"),
        "the bridge spawns the author's runtime through std"
    );
    assert!(
        !toml.contains("tokio") && !toml.contains("interprocess") && !toml.contains("quickjs"),
        "ForgeDB embeds no interpreter and takes no new dep to talk to one:\n{toml}"
    );
    assert!(
        !main.contains("forgedb_parser") && !main.contains("schema.forge"),
        "the emitted crate still reads no schema"
    );
}

#[test]
fn test_a_hop_without_an_escape_emits_no_bridge() {
    let (main, _) = sample_transform_crate();
    assert!(
        !main.contains("__Escape"),
        "the bridge is emitted only when a hop needs it:\n{main}"
    );
    let (with, _) = answered_transform_crate();
    assert!(
        with.contains("__Escape"),
        "…and IS emitted when one does — otherwise the assertion above is vacuous"
    );
}

#[test]
fn test_a_rust_escape_is_embedded_and_a_typescript_one_is_spawned() {
    let (rust_hop, crate_out) = sample_transform_crate();
    assert!(
        crate_out.sources.iter().any(|(p, _)| p == "src/authored_m2.rs"),
        "a Rust escape is embedded verbatim as a module (C13)"
    );
    assert!(!rust_hop.contains("__Escape::spawn"));

    let (ts_hop, crate_out) = answered_transform_crate();
    assert!(
        !crate_out
            .sources
            .iter()
            .any(|(p, _)| p.starts_with("src/authored_")),
        "a TypeScript escape embeds no Rust module"
    );
    let dense: String = ts_hop.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        dense.contains(r#"__Escape::spawn("/usr/bin/bun",&["/cache/escape/m1/transform.ts"],)"#),
        "both the interpreter and the script path are BAKED, not discovered at run \
         time:\n{ts_hop}"
    );
}
