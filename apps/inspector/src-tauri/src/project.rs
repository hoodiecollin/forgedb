use std::collections::HashSet;
use std::path::Path;

use forgedb_compaction::StatsCollector;
use forgedb_parser::{
    Constraint, ConstraintParam, Field as AstField, FieldType, Model as AstModel, Parser,
    RelationType, Struct as AstStruct,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub db_name: String,
    pub schema_path: String,
    pub data_dir: Option<String>,
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
    pub index_count: usize,
    pub fields: Vec<FieldDto>,
    pub stats: Option<ModelStatsDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructDto {
    pub name: String,
    pub fields: Vec<FieldDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDto {
    pub name: String,
    pub kind: String,
    pub auto: bool,
    pub unique: bool,
    pub indexed: bool,
    pub nullable: bool,
    pub bytes_len: Option<usize>,
    pub array_len: Option<usize>,
    pub fulltext: bool,
    pub computed: bool,
    pub materialized: bool,
    pub rel_target: Option<String>,
    pub struct_name: Option<String>,
    pub directives: Vec<DirectiveDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectiveDto {
    pub name: String,
    pub params: Vec<String>,
}

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

pub fn load_project(schema_path: &str, data_dir: Option<&str>) -> Result<ProjectDto, String> {
    let path = Path::new(schema_path);
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read schema {schema_path}: {e}"))?;

    let mut parser = Parser::new(&source).map_err(|e| format!("Parse error: {e}"))?;
    let schema = parser.parse().map_err(|e| format!("Parse error: {e}"))?;

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("schema");
    const GENERIC: [&str; 6] = ["schema", "db", "database", "model", "models", "main"];
    let db_name = if GENERIC.contains(&stem) {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty() && *s != ".")
            .unwrap_or(stem)
            .to_string()
    } else {
        stem.to_string()
    };

    let stats = data_dir
        .map(Path::new)
        .filter(|d| d.exists())
        .and_then(|d| StatsCollector::new(d).collect_database_stats().ok());
    let has_stats = stats.is_some();

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
        fields: s
            .fields
            .iter()
            .map(|f| field_dto(f, &s.name, &HashSet::new()))
            .collect(),
    }
}

fn field_dto(f: &AstField, owner: &str, m2m_fields: &HashSet<(String, String)>) -> FieldDto {
    let (inner, nullable_wrap) = match &f.field_type {
        FieldType::Nullable(inner) => (inner.as_ref(), true),
        other => (other, false),
    };

    let mut bytes_len = None;
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
        FieldType::String | FieldType::StringN { .. } => "string",
        FieldType::Json => "json",
        FieldType::Decimal => "decimal",
        FieldType::Uuid => "uuid",
        FieldType::Timestamp(_) => "timestamp",
        FieldType::Bytes(n) => {
            bytes_len = Some(*n);
            "bytes"
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
        FieldType::Enum(name) => {
            struct_name = Some(name.clone());
            "enum"
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
        FieldType::Nullable(_) => "string",
    };

    FieldDto {
        name: f.name.clone(),
        kind: kind.to_string(),
        auto: f.auto_generate,
        unique: f.unique,
        indexed: f.indexed,
        nullable,
        bytes_len,
        array_len,
        fulltext: f.fulltext_indexed,
        computed: f.is_computed,
        materialized: f.is_materialized,
        rel_target,
        struct_name,
        directives: f.constraints.iter().map(directive_dto).collect(),
    }
}

fn render_param(p: &ConstraintParam) -> String {
    match p {
        ConstraintParam::Number(n) => n.to_string(),
        ConstraintParam::Fractional(s) => s.clone(),
        ConstraintParam::String(s) => s.clone(),
        ConstraintParam::Named { name, value } => format!("{name}: {}", render_param(value)),
        ConstraintParam::Exclusive { greater, value } => {
            format!("{}{}", if *greater { '>' } else { '<' }, render_param(value))
        }
    }
}

fn directive_dto(c: &Constraint) -> DirectiveDto {
    DirectiveDto {
        name: c.name.clone(),
        params: c
            .params
            .iter()
            .map(|p| render_param(p))
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
                login_code: bytes(8)
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
        assert_eq!(code.kind, "bytes");
        assert_eq!(code.bytes_len, Some(8));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn generic_stem_uses_parent_dir_for_db_name() {
        let base =
            std::env::temp_dir().join(format!("fdb-insp-name-{}", std::process::id()));
        let proj = base.join("blog-cms");
        std::fs::create_dir_all(&proj).unwrap();
        let schema = write(&proj, "schema.forge", "User {\n  id: +uuid\n  name: string\n}\n");

        let dto = load_project(&schema, None).expect("load");
        assert_eq!(dto.db_name, "blog-cms");

        std::fs::remove_dir_all(&base).ok();
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
