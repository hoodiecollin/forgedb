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
