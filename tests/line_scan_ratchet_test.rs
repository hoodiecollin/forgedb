use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forgedb_source_guard::RustSource;

const ALLOWED: &[(&str, &str, usize, &str)] = &[];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rel(origin: &str) -> String {
    Path::new(origin)
        .strip_prefix(repo_root())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| origin.to_string())
}

fn test_sources() -> Vec<RustSource> {
    let root = repo_root();
    let mut out = RustSource::walk(root.join("tests"), &[]);
    for entry in std::fs::read_dir(root.join("crates")).expect("crates/ is readable") {
        let tests = entry.expect("dir entry").path().join("tests");
        if tests.is_dir() {
            out.extend(RustSource::walk(tests, &["snapshots"]));
        }
    }
    assert!(
        out.len() > 40,
        "walked only {} test files; the population below would be measured over almost nothing",
        out.len()
    );
    out
}

fn found_sites() -> BTreeMap<(String, String), usize> {
    let mut found = BTreeMap::new();
    for src in test_sources() {
        for (f, n) in src.method_call_sites("lines") {
            found.insert((rel(src.origin()), f), n);
        }
    }
    found
}

#[test]
fn every_line_scan_is_named_with_a_reason_or_it_fails() {
    let found = found_sites();
    let allowed: BTreeMap<(String, String), (usize, &str)> = ALLOWED
        .iter()
        .map(|(file, f, n, why)| ((file.to_string(), f.to_string()), (*n, *why)))
        .collect();

    let unlisted: Vec<String> = found
        .iter()
        .filter(|(k, n)| allowed.get(*k).map(|(m, _)| m) != Some(*n))
        .map(|((file, f), n)| format!("  {file}::{f}  ({n})"))
        .collect();
    let stale: Vec<String> = allowed
        .iter()
        .filter(|(k, _)| !found.contains_key(*k))
        .map(|((file, f), (n, _))| format!("  {file}::{f}  ({n})"))
        .collect();

    assert!(
        unlisted.is_empty(),
        "{} `.lines()` site(s) scan text that the allow-list does not name, or with a \
         different count. A line scan over Rust, Go or TOML reports clean the moment a reflow \
         moves the needle; ask the AST through forgedb_source_guard, the toml crate, or goguard. \
         If the text has no AST (process output, a .gitignore, a trace), add the site to ALLOWED \
         with its reason.\n{}",
        unlisted.len(),
        unlisted.join("\n")
    );
    assert!(
        stale.is_empty(),
        "{} allow-list entries name a site that no longer exists; remove them so the list \
         stays an equality and not a ceiling.\n{}",
        stale.len(),
        stale.join("\n")
    );
}

#[test]
fn the_detector_sees_a_single_line_chain_and_a_multi_line_chain_inside_a_macro() {
    let planted = RustSource::generated(
        "planted.rs",
        "fn a(t: &str) { assert!(t.lines().any(|l| l == \"x\")); }\n\
         fn b(t: &str) -> usize {\n    t\n        .lines()\n        .count()\n}\n",
    );
    let sites = planted.method_call_sites("lines");
    assert_eq!(
        sites.get("a"),
        Some(&1),
        "a single-line chain inside assert! must be seen; the text extractor missed this shape"
    );
    assert_eq!(sites.get("b"), Some(&1), "a multi-line chain must be seen");
}
