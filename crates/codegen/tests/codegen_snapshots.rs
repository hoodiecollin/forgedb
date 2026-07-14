//! Snapshot tests for code generation
//!
//! Uses insta for snapshot testing to ensure generated code remains stable.

use forgedb_codegen::{
    ApiGenerator, OpenApiGenerator, RustGenerator, TypeScriptGenerator, WasmGenerator,
};
use forgedb_parser::ast::{ComponentProtocol, ComponentReference, IndexType, RelationInclusion};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema};

/// Helper to create a simple test schema with one model
fn simple_user_schema() -> Schema {
    Schema {
        models: vec![Model {
            name: "User".to_string(),
            fields: vec![
                Field {
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
                Field {
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
                Field {
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
    }
}

/// Helper to create a schema with multiple models
fn multi_model_schema() -> Schema {
    Schema {
        models: vec![
            Model {
                name: "User".to_string(),
                fields: vec![Field {
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
            Model {
                name: "Post".to_string(),
                fields: vec![
                    Field {
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
                    Field {
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
    // Backed by the generated mutation surface. Create/update route through the
    // #91 Database-level validated wrappers (FK + field + unique); delete is direct.
    assert!(code.contains("db.update_user(key, record)"));
    assert!(code.contains("db.create_user(record)"));
    assert!(code.contains(".delete(key)"));
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
    // Phase 5 WS1: liveness/readiness/metrics handlers, the unauthenticated ops
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
        models: vec![Model {
            name: "ComplexModel".to_string(),
            fields: vec![
                Field {
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
                Field {
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
                Field {
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
                Field {
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
                Field {
                    name: "created_at".to_string(),
                    field_type: FieldType::Timestamp,
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
    };

    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

/// Helper to create a schema with complex fixed-size types
fn complex_types_schema() -> Schema {
    use forgedb_parser::Struct;
    
    Schema {
        structs: vec![
            Struct {
                name: "Address".to_string(),
                fields: vec![
                    Field {
                        name: "street".to_string(),
                        field_type: FieldType::Char(100),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "city".to_string(),
                        field_type: FieldType::Char(50),
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
            Struct {
                name: "Location".to_string(),
                fields: vec![
                    Field {
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
                    Field {
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
        models: vec![Model {
            name: "Place".to_string(),
            fields: vec![
                Field {
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
                Field {
                    name: "name".to_string(),
                    field_type: FieldType::Char(200),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
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
                Field {
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
                Field {
                    name: "tags".to_string(),
                    field_type: FieldType::FixedArray(Box::new(FieldType::Char(20)), 5),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
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
    assert!(code.contains("id_to_row: HashMap<u64, usize>"), "id map must be keyed by u64");
    // insert now returns Result<u64, _> (#91 validation); the PK type still threads through.
    assert!(code.contains("-> Result<u64, ValidationError>"), "insert must return the u64 PK");
    assert!(code.contains("id: u64"), "get must take the u64 PK");

    // Gap 1: nullable string field renders as Option<String> and is encoded
    // with a presence tag (so None and Some(\"\") stay distinct).
    assert!(code.contains("pub description: Option<String>"), "nullable string field");
    assert!(code.contains(r"String::from('\u{0}')"), "None must encode to a presence tag");

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
            Model {
                name: "Author".to_string(),
                fields: vec![Field {
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
            Model {
                name: "Post".to_string(),
                fields: vec![
                    Field {
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
                    Field {
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
                    Field {
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
                    Field {
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
    }
}

/// Schema where a model has a OneToMany virtual relation and a Component field.
fn component_schema() -> Schema {
    Schema {
        models: vec![Model {
            name: "Product".to_string(),
            fields: vec![
                Field {
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
                Field {
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
                Field {
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
                Field {
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

    // H1: URL uses correct template literal, id interpolated + URL-encoded (WS5).
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
    // WS5: full CRUD + typed error + pagination surface.
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
        models: vec![Model {
            name: "UserProfile".to_string(),
            fields: vec![Field {
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
        models: vec![Model {
            name: "Counter".to_string(),
            fields: vec![
                Field {
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
                Field {
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
        code.contains("db.id_to_row.insert(id, i);"),
        "reopen must rebuild id_to_row"
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
    // (`\u{1}` = Some, `\u{0}` = None) so None vs Some(Value::Null) stay distinct.
    assert!(
        code.contains("'\\u{1}'") && code.contains("'\\u{0}'"),
        "nullable json uses the presence-tag scheme"
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
    assert!(
        code.contains("save_to(&root.join(\"post/manifest.json\"))")
            || code.contains("save_to(& root.join(\"post/manifest.json\"))"),
        "model manifest must be written under the (root-joined) model directory"
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
    // as-of capture. Both bind the watermark once and track the newest row.
    assert!(
        code.contains("let watermark = snap.watermark();"),
        "snapshot accessors bind the watermark once"
    );
    assert!(
        code.contains("let mut newest: Option<usize> = None;"),
        "get_at must resolve the newest version within the watermark"
    );
    assert!(
        code.contains("let mut newest: HashMap<Uuid, usize> = HashMap::new();"),
        "all_at must resolve the newest version per id within the watermark"
    );
    // The junction still scans exactly the committed prefix (links are add-only).
    assert!(
        code.contains("(0..snap.watermark())"),
        "junction pairs_at must scan exactly the committed prefix"
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

    // The checkpoint fsyncs columns BEFORE truncating the WAL (the correctness
    // ordering) and resets the counter.
    assert!(
        code.contains("pub fn checkpoint(&mut self)"),
        "generated per-model checkpoint method"
    );
    assert!(
        code.contains("self.tombstones.flush()") && code.contains("self.wal.truncate()"),
        "checkpoint fsyncs columns/tombstones then truncates the WAL"
    );
    let flush_pos = code.find(".flush().expect(\"Failed to fsync tombstones on checkpoint\")");
    let trunc_pos = code.find(".truncate().expect(\"Failed to truncate WAL on checkpoint\")");
    assert!(
        matches!((flush_pos, trunc_pos), (Some(f), Some(t)) if f < t),
        "columns are fsync'd before the WAL is truncated (durability ordering)"
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

    // Junctions (no WAL — a #89 boundary) still fsync their id columns at checkpoint
    // so a full Database::checkpoint() leaves link rows as durable as model rows.
    assert!(
        code.contains("Failed to fsync junction left column on checkpoint"),
        "junction checkpoint fsyncs its id columns (no WAL to truncate)"
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

    // A per-field index structure keyed by canonical string -> set of ids.
    assert!(
        code.contains("email_index: std::collections::HashMap<String, std::collections::HashSet<Uuid>>")
            || code.contains("email_index : std :: collections :: HashMap"),
        "unique field gets a value->ids index"
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
    assert!(
        code.contains("self.email_index.entry(__k).or_default().insert(id);"),
        "insert/update maintain the index"
    );
    assert!(
        code.contains("self.email_index.get_mut(&__k)"),
        "update/delete remove stale index entries"
    );

    // Reopen rebuild is folded into the id-scan rehydrate (keyed off db.get).
    assert!(
        code.contains("db.email_index.entry(__k).or_default().insert(__id);"),
        "indexes are rebuilt from committed rows on reopen"
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
    assert!(code.contains("Unique { field") && code.contains("DanglingReference { field")
        && code.contains("Constraint { field"), "three integrity variants");
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
    assert!(code.contains("pub fn create_post(&mut self, record: Post) -> Result<Uuid, ValidationError>"),
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
    // Bounded storage (v1 Phase 4 — #92 Workstream 1): the mutation surface (#66)
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

    // Compaction renumbers rows → reopen to rebuild id_to_row + indexes.
    assert!(
        code.contains("*self = Self::new_at(&__root);"),
        "compact() reopens to rebuild the in-memory maps from compacted files"
    );

    // Auto-invoked from update AND delete (not insert — inserts create no dead
    // version) once the threshold is reached.
    assert!(
        code.contains("self.dead_since_compaction += 1;")
            && code.contains("if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD"),
        "update/delete count toward and trigger the auto-compaction"
    );

    // Database-wide force-compact across every MODEL (junctions excluded — an
    // append-only link table accumulates no dead versions).
    assert!(
        code.contains("self.widget.compact();") && code.contains("self.gadget.compact();"),
        "Database::compact() compacts every model collection"
    );
}

#[test]
fn test_rust_generation_additive_backfill() {
    // Additive migrations (v1 Phase 4 — #92 Workstream 2): after a field is added
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
        code.contains("append_string(&String::from('\\u{0}'))"),
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
    // Real list endpoint (#90): fetch live rows via all(), filter with the
    // generated closed-set matcher, sort with the generated per-model comparator,
    // paginate with the schema-agnostic query-params substrate.
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
    assert!(
        code.contains(".all()") && code.contains("user_event_matches(r, &params)"),
        "list fetches real rows and reuses the closed-set filter (no second parser)"
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
    let code = RustGenerator::generate(&schema).unwrap().code;
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
    assert!(
        flat.contains("self.post_tag_link.link(Uuid::from_bytes(__l),Uuid::from_bytes(__r))"),
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

    // Typed deltas + opaque membership.
    assert!(
        code.contains("PostLiveDelta::Init") || code.contains("PostLiveDelta :: Init"),
        "handler streams the generated typed delta enum"
    );
    assert!(
        code.contains("PostLiveDelta::Removed") || code.contains("PostLiveDelta :: Removed"),
        "handler emits removal-aware deltas"
    );
    assert!(
        code.contains("HashMap<Uuid, String>"),
        "membership tracks opaque id -> opaque hash"
    );
    // The relation field stays non-filterable (inherited from the shared filter).
    assert!(
        !code.contains("params.get(\"author\")"),
        "relation field is not a live-query filter key"
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

