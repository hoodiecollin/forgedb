//! Snapshot tests for code generation
//!
//! Uses insta for snapshot testing to ensure generated code remains stable.

use forgedb_codegen::{ApiGenerator, RustGenerator, TypeScriptGenerator};
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
    assert!(code.contains("-> u64"), "insert must return the u64 PK");
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
    assert!(
        code.contains("record.author == id"),
        "required-FK reverse getter filters by equality"
    );
    assert!(
        code.contains("record.editor == Some(id)"),
        "optional-FK reverse getter filters by Some(id)"
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

    // H1: URL uses correct template literal with ${id} interpolation
    assert!(code.contains("${id}"), "id interpolation missing in template literal");
    // H1: the old malformed pattern "{}" should NOT appear
    assert!(!code.contains("{}`"), "old malformed URL pattern still present");
    // H3: kebab-case (for "Post" it's "post"; verify by checking for single-word models)
    assert!(code.contains("/api/post/${id}"), "get URL for Post should be /api/post/${{id}}");
    assert!(code.contains("/api/author/${id}"), "get URL for Author should be /api/author/${{id}}");
    // M1: Uuid maps to string
    assert!(code.contains("id: string"), "Uuid should be string");

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

    // Every storage `new()` refreshes the physical-layout manifest on open.
    assert!(
        code.contains("db.write_manifest();"),
        "model new() must refresh the layout manifest on open"
    );
    assert!(
        code.contains("fn write_manifest(&self)"),
        "a write_manifest method must be generated"
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
        code.contains("save_to(std :: path :: Path :: new(\"post/manifest.json\"))")
            || code.contains("save_to(std::path::Path::new(\"post/manifest.json\"))"),
        "model manifest must be written under the model directory"
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
