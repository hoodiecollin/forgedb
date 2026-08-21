//! **Every crate the generated cache source names absolutely must be reachable
//! from the manifest that compiles it.**
//!
//! This guard exists because a missing dependency in a generated manifest is
//! invisible to everything else we run:
//!
//! * `insta` snapshots compare generated code as **strings**, so an absolute
//!   path to a crate nobody pinned looks exactly like one to a crate somebody
//!   did;
//! * an in-tree `cargo build --workspace` compiles the *generators*, never
//!   their output;
//! * and the failure is **schema-shaped** — `api.rs` only names
//!   `rust_decimal::Decimal` when some model carries a `decimal` field, so a
//!   fixture schema without one is green while every real app with a price
//!   column fails to build.
//!
//! It shipped exactly that way: `forgedb build` on a two-model schema with one
//! `decimal` column died with `error[E0433]: cannot find module or crate
//! rust_decimal` from `server/src/api.rs`, because `server/Cargo.toml` pinned
//! `serde`, `serde_json`, `utoipa`, `axum`, `tokio`… and not `rust_decimal`.
//!
//! # How it decides, and why that is not a hand-kept list
//!
//! The test reads the **emitted source** and extracts the first segment of every
//! path it writes — that is the token representing the work, and it moves when a
//! generator starts naming a new crate. It then requires each one to be
//! satisfied by exactly one of:
//!
//! 1. a dependency in that package's own generated `Cargo.toml`;
//! 2. a `pub use` in [`CORE_SUBSTRATE_REEXPORTS`], which the crate root globs in
//!    (`use forgedb_core::*;`) — this is how the wrappers pin zero substrate;
//! 3. the language itself (`std`/`core`/`alloc`, a primitive, `self`/`super`/`crate`);
//! 4. a sibling module of the crate being compiled (`api`, `database`).
//!
//! Nothing else is admitted, and in particular **there is no allowlist of
//! "crates we know are fine"** — that is the construct which would have let this
//! defect through, since `rust_decimal` reads as obviously fine.
//!
//! Note (2) really is different from (1): only a *direct dependency* enters a
//! crate's extern prelude. `core` re-exporting `rust_decimal` would answer
//! `forgedb_core::rust_decimal::Decimal`, not the bare `rust_decimal::Decimal`
//! the generated code writes — which is why the fix was a pin and not a re-export.

use forgedb::commands::generate::CORE_SUBSTRATE_REEXPORTS;
use forgedb_codegen::{ApiGenerator, GenConfig, ServerPackage};
use std::collections::BTreeSet;

/// Wide enough that `api.rs` reaches for every optional crate it can: a
/// `decimal` (the one that shipped broken), a `json`, a `uuid` identity, an
/// enum, an index, a unique, an optional column and both relation directions.
///
/// A narrower schema is how this defect stayed hidden — so the fixture is
/// deliberately maximal rather than minimal.
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

/// Rust's own names, which no manifest can or should carry.
const LANGUAGE: &[&str] = &[
    "std", "core", "alloc", "self", "super", "crate", "usize", "isize", "u8", "u16", "u32", "u64",
    "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64", "bool", "char", "str",
];

/// The first path segment of every path the source writes.
///
/// `use` statements are handled separately from the body: a grouped import
/// (`use axum::{extract::Path, http::StatusCode};`) puts inner segments at the
/// start of a line, and treating those as crate roots would make the test
/// demand a dependency named `http`. For a `use`, only the segment immediately
/// after the keyword is a crate root; everywhere else, an identifier followed by
/// `::` is one unless something binds it to the left (`.`, another `:`, or a
/// word character).
///
/// **A `use` is recognised only at the START of a line**, which is not
/// fussiness. The emitted `api.rs` is `prettyplease` output, so every real
/// import begins a line — while the CORS diagnostic it also emits contains the
/// English "use either a …" mid-string. Matching `use ` anywhere made the test
/// demand a dependency called `either`, and a guard that reports a crate nobody
/// wrote is a guard that gets disabled.
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
            // The rest of a grouped import can span lines; those continuation
            // lines carry no `use `, so they fall through to the body scan.
            // Feeding them in as-is would resurrect `http`/`extract`, so the
            // whole import block is dropped instead: an import's inner segments
            // are never crate roots.
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
            // The line that closes the import block.
            rest.push('\n');
            continue;
        }
        rest.push_str(line);
        rest.push('\n');
    }

    // Now the body: `ident::`, with nothing binding the ident to its left.
    let rb = rest.as_bytes();
    let mut j = 0usize;
    while j + 1 < rb.len() {
        if rb[j] == b':' && rb[j + 1] == b':' {
            // Walk back over the identifier.
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

/// Dependency names declared by a generated manifest, in cargo's spelling
/// (`-`), normalised to the extern-prelude spelling (`_`).
///
/// A renamed dependency (`forgedb_core = { package = "…", path = "…" }`) enters
/// the prelude under the KEY, which is what makes reading the keys the right
/// move here rather than reading the `package =` values.
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

/// Crates the crate root globs in from `core`'s re-exports.
fn reexported() -> BTreeSet<String> {
    CORE_SUBSTRATE_REEXPORTS
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub use "))
        .map(|l| l.trim_end_matches(';').trim().to_string())
        .collect()
}

/// **The guard.** Every crate `api.rs` and the server's `main.rs` name must be
/// reachable from `server/Cargo.toml` or through `core`'s re-exports.
#[test]
fn the_server_manifest_pins_every_crate_its_source_names() {
    let schema = forgedb_parser::Parser::new(SCHEMA)
        .and_then(|mut p| p.parse())
        .expect("fixture schema parses");
    let api = ApiGenerator::generate_with_config(&schema, GenConfig::DEFAULT).expect("api.rs");
    let main = ServerPackage::main_rs(forgedb_codegen::ServerLayout::Cache);
    let manifest = ServerPackage::cargo_toml("app-server", "app-core");

    let deps = manifest_deps(&manifest);
    let reexports = reexported();
    // `main.rs` renames the core dependency to `forgedb_core` and then
    // `use database::*`, so `database::` and `api::` are this crate's own
    // modules rather than crates.
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

/// The specific regression, named, so a future manifest rewrite that drops the
/// pin fails with the reason rather than with a generic list.
///
/// Two assertions, because they are two different claims: the source really does
/// write the bare path (if a generator ever routes it through `core`, this test
/// should be deleted, not weakened), and the manifest really does pin it.
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

/// The extractor must not be vacuous: if `crate_roots` returned nothing, the
/// guard above would pass on any manifest at all.
///
/// Asserted against known-present roots rather than a count, so it survives a
/// generator adding or dropping a crate.
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
    // Inner segments of a grouped `use` are NOT crate roots.
    for noise in ["extract", "http", "parse", "sync"] {
        assert!(
            !found.contains(noise),
            "`{noise}` was treated as a crate root: {found:?}"
        );
    }
}
