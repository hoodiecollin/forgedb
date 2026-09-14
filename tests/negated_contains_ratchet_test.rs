use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forgedb_source_guard::RustSource;

const BUDGET: &[(&str, usize)] = &[
    ("crates/codegen/tests/codegen_snapshots.rs", 194),
    ("crates/codegen/tests/default_fill_test.rs", 1),
    ("crates/migrations/tests/legacy_record_374.rs", 2),
    ("crates/source-guard/tests/detectors.rs", 2),
    ("crates/source-guard/tests/file_queries.rs", 2),
    ("crates/source-guard/tests/go_redline.rs", 1),
    ("crates/types/tests/inline_str_252.rs", 1),
    ("crates/validation/tests/lib_tests.rs", 1),
    ("tests/build_cache_compile_test.rs", 6),
    ("tests/build_driver_test.rs", 3),
    ("tests/cache_dir_test.rs", 8),
    ("tests/cache_home_isolation_test.rs", 1),
    ("tests/cache_manifest_deps_test.rs", 1),
    ("tests/ci_gate_test.rs", 17),
    ("tests/cli_loop_test.rs", 1),
    ("tests/delivery_test.rs", 9),
    ("tests/generation_memo_ratchet_test.rs", 1),
    ("tests/in_tree_package_test.rs", 3),
    ("tests/init_scaffold_test.rs", 4),
    ("tests/list_scan_test.rs", 1),
    ("tests/manifest_helpers_test.rs", 2),
    ("tests/migrate_answers_test.rs", 5),
    ("tests/migrate_capture_test.rs", 6),
    ("tests/migrate_tests.rs", 5),
    ("tests/page_identity_test.rs", 1),
    ("tests/placement_flip_test.rs", 4),
    ("tests/project_id_test.rs", 1),
    ("tests/project_identity_test.rs", 4),
    ("tests/prompt_boundary_test.rs", 8),
    ("tests/removed_surface_test.rs", 1),
    ("tests/semantic_search_test.rs", 1),
    ("tests/server_c4_guard_test.rs", 1),
    ("tests/snapshot_order_test.rs", 1),
];

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
    assert!(out.len() > 40, "walked only {} test files", out.len());
    out
}

#[test]
fn negated_substring_assertions_only_ever_decrease() {
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for src in test_sources() {
        let n = src.negated_method_call_count("contains");
        if n > 0 {
            found.insert(rel(src.origin()), n);
        }
    }
    let budget: BTreeMap<String, usize> = BUDGET
        .iter()
        .map(|(f, n)| (f.to_string(), *n))
        .collect();

    let mut diffs = Vec::new();
    for (file, n) in &found {
        match budget.get(file) {
            Some(b) if b == n => {}
            Some(b) => diffs.push(format!("  {file}: {n} negated contains, budget says {b}")),
            None => diffs.push(format!("  {file}: {n} negated contains, not in BUDGET")),
        }
    }
    for (file, b) in &budget {
        if !found.contains_key(file) {
            diffs.push(format!("  {file}: BUDGET says {b}, file has none (or no longer exists)"));
        }
    }

    assert!(
        diffs.is_empty(),
        "the negated-substring population moved and BUDGET was not updated to match.\n\
         `assert!(!text.contains(needle))` passes both when the construct is absent and when \
         the needle no longer spells anything the producer emits. Over generated or repo \
         source that is a silent false green, and codegen_snapshots.rs carries almost all of \
         those; over process output it is merely brittle. Each batch that migrates a file's \
         negated assertions to source-guard lowers its row; a new negated assertion needs a \
         deliberate raise, with a reason in the commit. Equality, not a ceiling, so slack \
         cannot hide a regression.\n{}",
        diffs.join("\n")
    );
    assert!(
        found.values().sum::<usize>() > 100,
        "the detector found almost nothing; the population is in the hundreds, so a \
         near-zero count means it stopped seeing inside assert! bodies"
    );
}

#[test]
fn the_detector_counts_a_negation_inside_assert_and_not_a_plain_assert() {
    let planted = RustSource::generated(
        "planted.rs",
        "fn a(code: &str) {\n    assert!(!code.contains(\"banned\"));\n    assert!(code.contains(\"kept\"));\n    assert!(!(code.contains(\"parens\")), \"msg\");\n    assert_eq!(code.contains(\"eq\"), false);\n}\n",
    );
    assert_eq!(planted.negated_method_call_count("contains"), 2);
}
