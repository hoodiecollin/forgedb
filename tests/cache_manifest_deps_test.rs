use forgedb::commands::generate::CORE_SUBSTRATE_REEXPORTS;
use forgedb_codegen::{ApiGenerator, GenConfig, ServerPackage};
use std::collections::BTreeSet;

const SCHEMA: &str = r#"
enum Status { Draft, Published, Archived }

Author {
  id: +uuid
  email: &string
  name: ^string
  posts: [Post]
}

Post {
  id: +uuid
  title: ^string
  body: string
  summary: string?
  views: u32
  price: decimal
  ratio: f64
  live: bool
  seen_at: timestamp
  status: ^Status
  meta: json
  author: *Author
  editor: ?Author
}
"#;

const LANGUAGE: &[&str] = &[
    "std", "core", "alloc", "self", "super", "crate", "usize", "isize", "u8", "u16", "u32", "u64",
    "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64", "bool", "char", "str",
];

fn crate_roots(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = String::new();

    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(after) = trimmed.strip_prefix("use ") {
            let head: String = after
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !head.is_empty() {
                found.insert(head);
            }
            if !trimmed.contains(';') {
                rest.push_str("\n@IMPORT_BLOCK\n");
            }
            continue;
        }
        if rest.ends_with("@IMPORT_BLOCK\n") && !trimmed.contains(';') {
            rest.push_str("@IMPORT_BLOCK\n");
            continue;
        }
        if rest.ends_with("@IMPORT_BLOCK\n") {
            rest.push('\n');
            continue;
        }
        rest.push_str(line);
        rest.push('\n');
    }

    let rb = rest.as_bytes();
    let mut j = 0usize;
    while j + 1 < rb.len() {
        if rb[j] == b':' && rb[j + 1] == b':' {
            let mut k = j;
            while k > 0 && (rb[k - 1].is_ascii_alphanumeric() || rb[k - 1] == b'_') {
                k -= 1;
            }
            if k < j {
                let ident = &rest[k..j];
                let bound_left = k > 0 && matches!(rb[k - 1], b'.' | b':');
                let looks_like_a_crate = ident
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
                if !bound_left && looks_like_a_crate {
                    found.insert(ident.to_string());
                }
            }
            j += 2;
            continue;
        }
        j += 1;
    }
    found
}

fn manifest_deps(manifest: &str) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t.contains("dependencies]");
            continue;
        }
        if !in_deps || t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some((key, _)) = t.split_once('=') {
            let key = key.trim().trim_matches('"');
            if !key.is_empty() {
                deps.insert(key.replace('-', "_"));
            }
        }
    }
    deps
}

fn reexported() -> BTreeSet<String> {
    CORE_SUBSTRATE_REEXPORTS
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub use "))
        .map(|l| l.trim_end_matches(';').trim().to_string())
        .collect()
}

#[test]
fn the_server_manifest_pins_every_crate_its_source_names() {
    let schema = forgedb_parser::Parser::new(SCHEMA)
        .and_then(|mut p| p.parse())
        .expect("fixture schema parses");
    let api = ApiGenerator::generate_with_config(&schema, GenConfig::DEFAULT).expect("api.rs");
    let main = ServerPackage::main_rs();
    let manifest = ServerPackage::cargo_toml("app-server", "app-core");

    let deps = manifest_deps(&manifest);
    let reexports = reexported();
    let own_modules: BTreeSet<String> = ["api", "database"].iter().map(|s| s.to_string()).collect();

    let mut missing: Vec<(&str, String)> = Vec::new();
    for (where_, source) in [("server/src/api.rs", &api.code), ("server/src/main.rs", &main)] {
        for root in crate_roots(source) {
            if LANGUAGE.contains(&root.as_str())
                || own_modules.contains(&root)
                || deps.contains(&root)
                || reexports.contains(&root)
            {
                continue;
            }
            missing.push((where_, root));
        }
    }

    assert!(
        missing.is_empty(),
        "generated server source names crates nothing makes reachable.\n\
         Fix by adding the dependency to `ServerPackage::cargo_toml` \
         (crates/codegen/src/server_pkg.rs) — NOT by re-exporting it from `core`, \
         which would only answer `forgedb_core::<name>` and not the bare path the \
         generated code writes.\n\n\
         {}\n\n\
         server/Cargo.toml pins: {:?}\n\
         core re-exports:        {:?}",
        missing
            .iter()
            .map(|(w, r)| format!("  {w}: `{r}::…` is not pinned and not re-exported"))
            .collect::<Vec<_>>()
            .join("\n"),
        deps,
        reexports
    );
}

#[test]
fn a_decimal_column_makes_the_server_name_rust_decimal_and_the_manifest_pin_it() {
    let schema = forgedb_parser::Parser::new(SCHEMA)
        .and_then(|mut p| p.parse())
        .expect("fixture schema parses");
    let api = ApiGenerator::generate_with_config(&schema, GenConfig::DEFAULT).expect("api.rs");

    assert!(
        api.code.contains("rust_decimal::Decimal"),
        "the REST filter no longer parses a decimal by absolute path — if it now \
         goes through `core`, delete this test rather than relaxing it"
    );
    assert!(
        manifest_deps(&ServerPackage::cargo_toml("app-server", "app-core")).contains("rust_decimal"),
        "server/Cargo.toml stopped pinning rust_decimal; `forgedb build` fails with \
         E0433 for every schema carrying a `decimal` column"
    );
}

#[test]
fn the_extractor_finds_the_roots_it_is_supposed_to() {
    let found = crate_roots(
        "use axum::{extract::Path, http::StatusCode};\n\
         use std::sync::Arc;\n\
         fn f() { let x = want.parse::<rust_decimal::Decimal>(); serde_json::json!({}); }\n\
         const N: usize = usize::MAX;\n",
    );
    for expected in ["axum", "std", "rust_decimal", "serde_json", "usize"] {
        assert!(found.contains(expected), "missed {expected}: {found:?}");
    }
    for noise in ["extract", "http", "parse", "sync"] {
        assert!(
            !found.contains(noise),
            "`{noise}` was treated as a crate root: {found:?}"
        );
    }
}
