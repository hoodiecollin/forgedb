//! At-rest **Structure lens** (#12): parse a project's `.forge` schema and read
//! its on-disk storage stats, then hand the frontend a faithful, schema-shaped
//! DTO. Everything here is *tooling* — the parser produces the same AST the CLI
//! uses, and [`forgedb_compaction::StatsCollector`] reads the data directory's
//! physical layout (tombstones + column files as opaque bytes). No runtime schema
//! engine, no schema-as-runtime-input: the identity red line stays intact.
//!
//! The DTO stays close to the AST — presentation choices (which editor control a
//! field maps to, how `@min`/`@max` render) live in the frontend, per the design
//! review. Rust reports facts; TypeScript renders them.

use std::collections::HashSet;
use std::path::Path;

use forgedb_compaction::StatsCollector;
use forgedb_parser::{
    Constraint, ConstraintParam, Field as AstField, FieldType, Model as AstModel, Parser,
    RelationType, Struct as AstStruct,
};
use serde::Serialize;

/// The whole loaded project, as the Structure lens consumes it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    /// Display name — the schema file stem (e.g. `blog` for `blog.forge`).
    pub db_name: String,
    pub schema_path: String,
    pub data_dir: Option<String>,
    /// True when a data dir was supplied and yielded per-model stats.
    pub has_stats: bool,
    pub models: Vec<ModelDto>,
    pub structs: Vec<StructDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub name: String,
    pub soft_delete: bool,
    pub composite_indexes: Vec<Vec<String>>,
    /// Count of `^`-indexed scalar fields — the storage panel's index count.
    pub index_count: usize,
    pub fields: Vec<FieldDto>,
    /// Physical storage stats — `None` when no data dir has this model yet.
    pub stats: Option<ModelStatsDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructDto {
    pub name: String,
    pub fields: Vec<FieldDto>,
}

/// One field, reduced to the raw facts a control needs. `kind` is a normalized
/// discriminant; the frontend maps it (plus the flags) to a `FieldControl` and a
/// `typeLabel`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDto {
    pub name: String,
    /// Normalized type discriminant: one of `u32 u64 i32 i64 f64 bool string uuid
    /// timestamp char fixed_array struct required_ref optional_ref one_to_many
    /// many_to_many component`.
    pub kind: String,
    pub auto: bool,
    pub unique: bool,
    pub indexed: bool,
    pub nullable: bool,
    /// `char(N)` length.
    pub char_len: Option<usize>,
    /// `[T; N]` fixed-array length.
    pub array_len: Option<usize>,
    pub fulltext: bool,
    pub computed: bool,
    pub materialized: bool,
    /// Target model for a relation field.
    pub rel_target: Option<String>,
    /// Referenced struct name for a struct field.
    pub struct_name: Option<String>,
    pub directives: Vec<DirectiveDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectiveDto {
    pub name: String,
    /// Directive args as strings (numbers stringified) — semantic-only markers.
    pub params: Vec<String>,
}

/// Physical storage stats for one model, mirrored from
/// [`forgedb_compaction::types::ModelStats`]. These are *physical* counts over the
/// append-only files: `active_rows` is non-tombstoned row versions (with the #66
/// mutation surface, superseded versions inflate this until compaction) and
/// `dead_*` is reclaimable space. Honest storage-health numbers, not logical
/// distinct-record counts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatsDto {
    pub total_rows: usize,
    pub active_rows: usize,
    pub deleted_rows: usize,
    pub dead_space_ratio: f64,
    pub total_disk_bytes: u64,
    pub used_bytes: u64,
    pub dead_bytes: u64,
}

/// Parse the schema at `schema_path`; if `data_dir` is given and exists, also
/// read per-model storage stats. Returns a JSON-ready DTO or a human-readable
/// error string (surfaced verbatim in the frontend's open-project flow).
pub fn load_project(schema_path: &str, data_dir: Option<&str>) -> Result<ProjectDto, String> {
    let path = Path::new(schema_path);
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read schema {schema_path}: {e}"))?;

    let mut parser = Parser::new(&source).map_err(|e| format!("Parse error: {e}"))?;
    let schema = parser.parse().map_err(|e| format!("Parse error: {e}"))?;

    let db_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("schema")
        .to_string();

    // Stats are additive: absent/empty data dir ⇒ schema-only structure view.
    let stats = data_dir
        .map(Path::new)
        .filter(|d| d.exists())
        .and_then(|d| StatsCollector::new(d).collect_database_stats().ok());
    let has_stats = stats.is_some();

    // (model, field) pairs that are true many-to-many (bidirectional [..]/[..]),
    // so we can tell an M2M collection apart from a plain one-to-many.
    let m2m_fields: HashSet<(String, String)> = schema
        .detect_many_to_many_relations()
        .into_iter()
        .flat_map(|r| {
            [
                (r.model1.clone(), r.field1.clone()),
                (r.model2.clone(), r.field2.clone()),
            ]
        })
        .collect();

    let models = schema
        .models
        .iter()
        .map(|m| model_dto(m, &m2m_fields, stats.as_ref()))
        .collect();

    let structs = schema.structs.iter().map(struct_dto).collect();

    Ok(ProjectDto {
        db_name,
        schema_path: schema_path.to_string(),
        data_dir: data_dir.map(str::to_string),
        has_stats,
        models,
        structs,
    })
}

fn model_dto(
    model: &AstModel,
    m2m_fields: &HashSet<(String, String)>,
    stats: Option<&forgedb_compaction::types::DatabaseStats>,
) -> ModelDto {
    let fields: Vec<FieldDto> = model
        .fields
        .iter()
        .map(|f| field_dto(f, &model.name, m2m_fields))
        .collect();

    let index_count = model.fields.iter().filter(|f| f.indexed).count();

    let model_stats = stats
        .and_then(|db| db.models.iter().find(|s| s.name == model.name))
        .map(|s| ModelStatsDto {
            total_rows: s.total_rows,
            active_rows: s.active_rows,
            deleted_rows: s.deleted_rows,
            dead_space_ratio: s.dead_space_ratio,
            total_disk_bytes: s.total_disk_bytes,
            used_bytes: s.used_bytes,
            dead_bytes: s.dead_bytes,
        });

    ModelDto {
        name: model.name.clone(),
        soft_delete: model.soft_delete,
        composite_indexes: model
            .composite_indexes
            .iter()
            .map(|ci| ci.fields.clone())
            .collect(),
        index_count,
        fields,
        stats: model_stats,
    }
}

fn struct_dto(s: &AstStruct) -> StructDto {
    StructDto {
        name: s.name.clone(),
        // Struct fields are always fixed-size scalars; no relation context needed.
        fields: s
            .fields
            .iter()
            .map(|f| field_dto(f, &s.name, &HashSet::new()))
            .collect(),
    }
}

fn field_dto(f: &AstField, owner: &str, m2m_fields: &HashSet<(String, String)>) -> FieldDto {
    // Peel a `Nullable(_)` wrapper so `kind` is the underlying type and the
    // nullability is a flag (the frontend renders a null toggle from it).
    let (inner, nullable_wrap) = match &f.field_type {
        FieldType::Nullable(inner) => (inner.as_ref(), true),
        other => (other, false),
    };

    let mut char_len = None;
    let mut array_len = None;
    let mut rel_target = None;
    let mut struct_name = None;
    let mut nullable = nullable_wrap;

    let kind = match inner {
        FieldType::U32 => "u32",
        FieldType::U64 => "u64",
        FieldType::I32 => "i32",
        FieldType::I64 => "i64",
        FieldType::F64 => "f64",
        FieldType::Bool => "bool",
        FieldType::String => "string",
        FieldType::Uuid => "uuid",
        FieldType::Timestamp => "timestamp",
        FieldType::Char(n) => {
            char_len = Some(*n);
            "char"
        }
        FieldType::FixedArray(_, n) => {
            array_len = Some(*n);
            "fixed_array"
        }
        FieldType::StructType(name) => {
            struct_name = Some(name.clone());
            "struct"
        }
        FieldType::OptionalStructType(name) => {
            struct_name = Some(name.clone());
            nullable = true;
            "struct"
        }
        FieldType::Relation(rel) => {
            rel_target = Some(rel.target_model().to_string());
            match rel {
                RelationType::RequiredReference(_) => "required_ref",
                RelationType::OptionalReference(_) => {
                    nullable = true;
                    "optional_ref"
                }
                RelationType::ManyToMany(_) => "many_to_many",
                RelationType::OneToMany(_) => {
                    if m2m_fields.contains(&(owner.to_string(), f.name.clone())) {
                        "many_to_many"
                    } else {
                        "one_to_many"
                    }
                }
            }
        }
        FieldType::Component(_) => "component",
        // Nullable is already peeled above; a doubly-nested Nullable is invalid.
        FieldType::Nullable(_) => "string",
    };

    FieldDto {
        name: f.name.clone(),
        kind: kind.to_string(),
        auto: f.auto_generate,
        unique: f.unique,
        indexed: f.indexed,
        nullable,
        char_len,
        array_len,
        fulltext: f.fulltext_indexed,
        computed: f.is_computed,
        materialized: f.is_materialized,
        rel_target,
        struct_name,
        directives: f.constraints.iter().map(directive_dto).collect(),
    }
}

fn directive_dto(c: &Constraint) -> DirectiveDto {
    DirectiveDto {
        name: c.name.clone(),
        params: c
            .params
            .iter()
            .map(|p| match p {
                ConstraintParam::Number(n) => n.to_string(),
                ConstraintParam::String(s) => s.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn parses_schema_only_without_data_dir() {
        let tmp = std::env::temp_dir().join(format!("fdb-insp-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let schema = write(
            &tmp,
            "blog.forge",
            r#"
            Org {
                id: +uuid
                name: &string
            }
            User {
                id: +uuid
                email: &^string @email
                age: i32? @min(0) @max(120)
                org: *Org
                bio: string? @length(0, 280)
                login_code: char(8)
            }
            "#,
        );

        let dto = load_project(&schema, None).expect("load");
        assert_eq!(dto.db_name, "blog");
        assert!(!dto.has_stats);
        assert_eq!(dto.models.len(), 2);

        let user = dto.models.iter().find(|m| m.name == "User").unwrap();
        assert!(user.stats.is_none());

        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        assert_eq!(email.kind, "string");
        assert!(email.unique && email.indexed);
        assert!(email.directives.iter().any(|d| d.name == "email"));

        let age = user.fields.iter().find(|f| f.name == "age").unwrap();
        assert_eq!(age.kind, "i32");
        assert!(age.nullable);
        let max = age.directives.iter().find(|d| d.name == "max").unwrap();
        assert_eq!(max.params, vec!["120"]);

        let org = user.fields.iter().find(|f| f.name == "org").unwrap();
        assert_eq!(org.kind, "required_ref");
        assert_eq!(org.rel_target.as_deref(), Some("Org"));

        let code = user.fields.iter().find(|f| f.name == "login_code").unwrap();
        assert_eq!(code.kind, "char");
        assert_eq!(code.char_len, Some(8));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn reads_storage_stats_when_data_dir_present() {
        let tmp = std::env::temp_dir().join(format!("fdb-insp-stats-{}", std::process::id()));
        let data = tmp.join("data");
        std::fs::create_dir_all(&tmp).unwrap();
        let schema = write(
            &tmp,
            "blog.forge",
            "User {\n  id: +uuid\n  name: string\n}\n",
        );

        // Fabricate the physical layout StatsCollector reads (schema-blind): a
        // `User` model dir with 5 committed rows, 1 tombstoned. tombstones.bin is
        // the row anchor (1 byte/row); a fixed uuid column is 16 bytes/row.
        let model = data.join("User");
        std::fs::create_dir_all(model.join("fixed")).unwrap();
        std::fs::write(model.join("tombstones.bin"), [0u8, 0, 1, 0, 0]).unwrap();
        std::fs::write(model.join("fixed").join("uuid_0.bin"), [0u8; 80]).unwrap();

        let dto = load_project(&schema, data.to_str()).expect("load");
        assert!(dto.has_stats);
        let user = dto.models.iter().find(|m| m.name == "User").unwrap();
        let stats = user.stats.as_ref().expect("stats present");
        assert_eq!(stats.total_rows, 5);
        assert_eq!(stats.deleted_rows, 1);
        assert_eq!(stats.active_rows, 4);
        assert!(stats.dead_space_ratio > 0.0);

        std::fs::remove_dir_all(&tmp).ok();
    }
}
