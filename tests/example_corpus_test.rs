use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use forgedb::commands::validate::parse_and_validate;
use forgedb_parser::ast::{FieldType, Schema, TimestampPrecision};

mod common;
use common::cache::Fixture;

const CONFIG: &str = r#"[project]
id = "example-corpus"
isolated = true

[generate]
targets = ["rust", "api"]
"#;

const SHOWCASED: &[&str] = &[
    "decimal",
    "json",
    "timestamp(us)",
    "bytes(N)",
    "string(N)",
    "[T; N]",
    "struct",
    "enum",
];

fn example_schemas() -> Vec<(PathBuf, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("examples/ is readable") {
        let schema = entry.expect("dir entry").path().join("schema.forge");
        if schema.is_file() {
            let content = std::fs::read_to_string(&schema).expect("read schema.forge");
            out.push((schema, content));
        }
    }
    out.sort();
    assert!(
        out.len() >= 18,
        "found {} example schemas under examples/; the corpus has 18 and a smaller count \
         means the walk is looking in the wrong place",
        out.len()
    );
    out
}

fn tag_of(ty: &FieldType) -> Option<&'static str> {
    match ty {
        FieldType::Nullable(inner) => tag_of(inner),
        FieldType::Decimal => Some("decimal"),
        FieldType::Json => Some("json"),
        FieldType::Timestamp(TimestampPrecision::Micros) => Some("timestamp(us)"),
        FieldType::Bytes(_) => Some("bytes(N)"),
        FieldType::StringN { .. } => Some("string(N)"),
        FieldType::FixedArray(..) => Some("[T; N]"),
        FieldType::StructType(_) | FieldType::OptionalStructType(_) => Some("struct"),
        FieldType::Enum(_) => Some("enum"),
        _ => None,
    }
}

fn type_tags(schema: &Schema) -> BTreeSet<&'static str> {
    schema
        .models
        .iter()
        .flat_map(|m| m.fields.iter())
        .filter_map(|f| tag_of(&f.field_type))
        .collect()
}

fn parsed(label: &str, content: &str) -> Schema {
    let parsed =
        parse_and_validate(content).unwrap_or_else(|e| panic!("{label}: fatal lexer error: {e}"));
    assert!(
        parsed.diagnostics.is_empty(),
        "{label}: an example must be clean before its types count: {:?}",
        parsed.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    parsed.schema
}

#[test]
fn every_example_generates() {
    let schemas = example_schemas();
    let mut generated = 0usize;
    for (path, content) in &schemas {
        let _fx = Fixture::generate(CONFIG, &[("schema.forge", content)]);
        generated += 1;
        let _ = path;
    }
    assert_eq!(
        generated,
        schemas.len(),
        "every example schema must generate; the fixture panics on the first that does not"
    );
}

const WIRE_CONFIG: &str = r#"[project]
id = "example-corpus-wire"
isolated = true

[generate]
targets = ["rust", "api", "openapi"]
"#;

#[test]
fn the_openapi_document_and_the_handler_agree_on_every_write_body() {
    let mut omitted_total = 0usize;
    let mut undefaulted: Vec<String> = Vec::new();

    for (path, content) in example_schemas() {
        let label = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let schema = parsed(&label, &content);
        let fx = Fixture::generate(WIRE_CONFIG, &[("schema.forge", &content)]);
        let generated = fx.project_dir().join("generated");
        let database = forgedb_source_guard::RustSource::repo_file(generated.join("database.rs"));
        let openapi: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(generated.join("openapi.json")).expect("openapi.json emitted"),
        )
        .expect("openapi.json parses");

        for model in &schema.models {
            let documented: Vec<String> = openapi["components"]["schemas"][&model.name]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{label}: openapi.json has no schema for {}", model.name))
                .keys()
                .cloned()
                .collect();
            let generated_struct = database.struct_named(&model.name).unwrap_or_else(|e| {
                panic!("{label}: database.rs has no struct for {}: {e}", model.name)
            });
            for field in &generated_struct.fields {
                let name = field.ident.as_ref().expect("named field").to_string();
                if documented.contains(&name) {
                    continue;
                }
                omitted_total += 1;
                let attrs = database.field_attrs(&model.name, &name).expect("field exists");
                if !attrs.iter().any(|a| a.starts_with("serde") && a.contains("default")) {
                    undefaulted.push(format!("{label}: {}.{name}", model.name));
                }
            }
        }
    }

    assert!(
        omitted_total > 0,
        "no example model has a field the openapi document omits, so this guard compared \
         nothing; the corpus has dozens of virtual-relation fields"
    );
    assert!(
        undefaulted.is_empty(),
        "{} field(s) are absent from the create/replace body openapi.json documents yet \
         REQUIRED by the generated handler's deserializer, so the documented body is a 422. \
         The struct field needs #[serde(default)] (#286):\n  {}",
        undefaulted.len(),
        undefaulted.join("\n  ")
    );
}

#[test]
fn the_corpus_shows_every_showcased_type() {
    let mut where_seen: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut micros_identity = Vec::new();

    for (path, content) in example_schemas() {
        let label = path.display().to_string();
        let schema = parsed(&label, &content);
        for tag in type_tags(&schema) {
            where_seen.entry(tag).or_default().push(label.clone());
        }
        for model in &schema.models {
            if let Some(id) = model.identity_field()
                && id.field_type == FieldType::Timestamp(TimestampPrecision::Micros)
            {
                micros_identity.push(format!("{label}: {}", model.name));
            }
        }
    }

    let missing: Vec<&str> = SHOWCASED
        .iter()
        .copied()
        .filter(|t| !where_seen.contains_key(t))
        .collect();
    assert!(
        missing.is_empty(),
        "no example schema uses {missing:?}. examples/ is the published demonstration of the \
         schema language and docs/SCHEMA.md documents every one of these; a reader working \
         through the corpus never meets them. Seen so far:\n{}",
        where_seen
            .iter()
            .map(|(t, files)| format!("  {t:<14} {}", files.len()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        !micros_identity.is_empty(),
        "no example model is keyed by `id: +timestamp(us)`. The identity form has rules of \
         its own (named id, declared us) that the precision form does not exercise"
    );
}
