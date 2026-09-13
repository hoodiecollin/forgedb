use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forgedb_source_guard::RustSource;

const ALLOWED: &[(&str, &str, usize, &str)] = &[
    ("tests/cli_loop_test.rs", "print_artifact_stdout_is_exactly_one_line", 1, "stdout of forgedb build --print-artifact"),
    ("tests/snapshot_order_test.rs", "order_line", 1, "stdout of the compiled snapshot-order driver"),
    ("tests/in_tree_package_test.rs", "printed_dep_line", 1, "stdout of forgedb generate"),
    ("tests/in_tree_package_test.rs", "scenarios_9_and_17_build_regenerates_the_package_but_never_plans_it", 1, "stdout of forgedb build --plan"),
    ("tests/build_cache_compile_test.rs", "defined", 1, "stdout of nm"),
    ("tests/migrate_tests.rs", "test_migrate_build_reports_the_path_cargo_actually_wrote", 1, "stdout of forgedb migrate build"),
    ("tests/project_identity_test.rs", "project_line", 1, "stdout of forgedb"),
    ("tests/delivery_test.rs", "scenario_15_nothing_delivered_depends_on_the_cache", 1, "stdout of otool -D"),
    ("tests/delivery_test.rs", "defined_symbols", 1, "stdout of nm"),
    ("tests/common/mod.rs", "parse_linked_libraries", 1, "stdout of otool -L / ldd"),
    ("crates/codegen/tests/doc_examples.rs", "generate_rust_from_a_parsed_schema_compiles", 1, "a discarded line count; asserts nothing"),
    ("tests/generation_memo_ratchet_test.rs", "generation_sites", 1, "one line of context for a failure listing"),
    ("crates/codegen/tests/codegen_snapshots.rs", "test_ffi_cache_package_manifest", 1, "the first 40 lines for a failure message"),
    ("tests/placement_flip_test.rs", "scenario_31_the_go_preamble_links_the_delivered_archive_statically", 1, "the first 40 lines for a failure message"),
    ("tests/ci_gate_test.rs", "make_recipe", 1, "the Makefile; no parser worth adding for a handful of targets"),
    ("tests/ci_gate_test.rs", "workflow", 1, "workflow YAML comment stripping; no YAML parser in the dependency graph, and the guards assert on shell inside run: scalars"),
    ("tests/ci_gate_test.rs", "trigger_block", 1, "workflow YAML on: block"),
    ("tests/ci_gate_test.rs", "run_block", 1, "a workflow step's run: | shell script"),
    ("tests/ci_gate_test.rs", "every_registry_resolving_job_runs_on_main_only", 1, "workflow YAML branches: lines"),
    ("tests/ci_gate_test.rs", "no_reclose_workflow_passes_a_tombstoned_cli_flag", 1, "shell lines invoking $FORGEDB in a workflow"),
    ("tests/ci_gate_test.rs", "run_block_returns_the_whole_script_not_the_scalar_header", 1, "line count of an extracted shell script"),
    ("tests/ci_gate_test.rs", "the_parent_workspace_job_still_does_its_work", 1, "shell lines of a workflow step"),
    ("tests/ci_gate_test.rs", "s337_the_go_reclose_proves_the_init_check_executes", 2, "the TOML template's comment lines (read from a string literal via the AST) and a workflow step's shell"),
    ("tests/semantic_search_test.rs", "the_serena_config_is_complete_so_serena_does_not_rewrite_it_on_every_start", 2, ".serena/project.yml; no YAML parser in the dependency graph"),
    ("tests/init_scaffold_test.rs", "the_dockerfile_drives_the_cli_and_copies_the_reported_artifact", 1, "a Dockerfile"),
    ("tests/init_scaffold_test.rs", "the_gitignore_no_longer_ignores_generated_wholesale", 2, "a .gitignore"),
    ("tests/init_scaffold_test.rs", "s17_init_mints_a_unique_committed_id", 1, "a .gitignore"),
    ("tests/delivery_test.rs", "scenario_8_ignoring_lives_in_exactly_one_place", 1, "a .gitignore"),
    ("tests/delivery_test.rs", "scenario_9_generated_code_carries_no_version_string_and_no_timestamp", 1, "a lexical word sweep over nine artifacts in five languages; language-agnostic by design"),
    ("tests/prompt_boundary_test.rs", "trace_lines", 1, "the FORGEDB_ASK_TRACE file"),
    ("crates/codegen/tests/codegen_snapshots.rs", "test_wasm_generation_async_client_and_worker", 1, "generated TypeScript; no TS parser in the dependency graph"),
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
