//! In-tree Rust placement — `[placement].rust_package` (#338, epic #332 class D).
//!
//! Gate 2 (#356) numbers fourteen scenarios; the ones that do not compile a
//! crate live here and run by default. The three that build a real consumer
//! workspace (5, 11c, 16b) are `#[ignore]`d at the bottom of this file and run
//! under `make test-ignored` — they are the only end-to-end proof the feature
//! has, and a build-only check cannot substitute for running them.
//!
//! Everything here drives the real `forgedb` binary as a subprocess with an
//! explicit `current_dir` and its own `FORGEDB_HOME`, so the cases are hermetic
//! and run in parallel — the convention `tests/placement_flip_test.rs` already
//! follows. The `FORGEDB_HOME` override is correctness, not hygiene: without it
//! `generate` claims a project id in the developer's real `~/.forgedb` ledger.
//!
//! **What the tier-2 coverage does and does not prove.** The compiling
//! scenarios build the consumer workspace with a `[patch.crates-io]` block
//! pointing at this checkout, so they prove the emitted package **compiles**.
//! They say nothing about **registry resolution** — a patch table is precisely
//! the thing that makes a registry lookup not happen. The only check that proves
//! an installed user can build is the outside-repo reclose on `main`
//! (`.github/workflows/substrate-reclose.yml`), which does not yet have an
//! in-tree arm. That gap is #339's, and it is a row on release gate #378.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SCHEMA: &str = r#"
Author {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: ^string
  body: string
  author: *Author
}
"#;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A fresh project directory holding a schema and a `forgedb.toml`.
///
/// `placement` is the whole `[placement]` table, or the empty string for the
/// opt-out — which is what makes scenario 1 and scenario 2 the same fixture with
/// one difference.
fn project(tag: &str, targets: &str, placement: &str) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write(&tmp.path().join("schema.forge"), SCHEMA);
    write(
        &tmp.path().join("forgedb.toml"),
        &format!(
            "[project]\nid = \"{tag}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n{placement}"
        ),
    );
    tmp
}

fn forgedb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .args(args)
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".home"))
        .output()
        .expect("run forgedb")
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn ok(out: &Output, what: &str) -> String {
    assert!(out.status.success(), "{what} failed:\n{}", combined(out));
    combined(out)
}

/// The app's single container under the cache, found by SCANNING rather than by
/// recomputing the member hash — a second derivation of the hash in a test is a
/// way for the test to agree with itself while disagreeing with the CLI.
fn container(root: &Path, name: &str) -> PathBuf {
    let apps = root.join(".home/projects").join(name).join("apps");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no cache at {}: {e}", apps.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    assert_eq!(found.len(), 1, "expected exactly one app container: {found:?}");
    found.pop().unwrap()
}

/// Every file under `dir`, as paths relative to it, sorted.
fn tree(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The `[package] name` a cargo manifest declares.
fn package_name(manifest: &Path) -> String {
    let body = read(manifest);
    let value: toml::Value = toml::from_str(&body)
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}\n{body}", manifest.display()));
    value["package"]["name"]
        .as_str()
        .expect("[package] name")
        .to_string()
}

/// The dep line the CLI printed, stripped of the `ui::info` decoration.
///
/// Found by the TOML key rather than by position: an added log line must not
/// silently re-point this at something else.
fn printed_dep_line(output: &str) -> String {
    let hits: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("forgedb_core = {"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one printed dep line, got {hits:?}\n--- output ---\n{output}"
    );
    let line = hits[0];
    let start = line
        .find("forgedb_core = {")
        .expect("just matched on it, so it is there");
    line[start..].trim_end().to_string()
}

const PLACEMENT: &str = "\n[placement]\nrust_package = \"generated/core\"\n";

// ===========================================================================
// Scenario 1 — absence is the opt-out
// ===========================================================================

/// **Scenario 1.** With no `[placement]` table, `generate all --force` writes no
/// cargo package anywhere in the user's tree.
///
/// The cache assertions are the other half and are not decoration: "nothing was
/// written" must not be satisfiable by generation having silently done nothing.
#[test]
fn scenario_1_absence_of_the_table_emits_no_package() {
    let tmp = project("s1", "\"rust\", \"api\"", "");
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let stray: Vec<String> = tree(&root.join("generated"))
        .into_iter()
        .filter(|p| p.ends_with("Cargo.toml"))
        .collect();
    assert!(
        stray.is_empty(),
        "an opted-out project got a cargo manifest in its output directory: {stray:?}"
    );
    assert!(
        !root.join("generated/core").exists(),
        "an opted-out project got a core/ directory"
    );

    // Generation really ran, and the cache package is unaffected by the feature.
    let cache = container(root, "s1");
    assert!(cache.join("core/Cargo.toml").is_file());
    assert!(cache.join("core/src/lib.rs").is_file());
    assert!(root.join("generated/database.rs").is_file());
}

// ===========================================================================
// Scenario 2 — the knob emits a complete package
// ===========================================================================

/// **Scenario 2.** The knob emits `Cargo.toml` + `src/lib.rs`, the manifest
/// parses as TOML and declares `edition = "2024"`, and the directory holds
/// **nothing else**.
///
/// The exhaustive file list is the load-bearing assertion. "Contains a
/// Cargo.toml" would pass for a directory that also held a `main.rs`, an
/// `api.rs`, or a leftover from a previous shape.
#[test]
fn scenario_2_the_knob_emits_a_complete_package() {
    let tmp = project("s2", "\"rust\", \"api\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let pkg = root.join("generated/core");
    assert_eq!(
        tree(&pkg),
        vec!["Cargo.toml".to_string(), "src/lib.rs".to_string()],
        "the in-tree package must hold exactly the two files a `core` package is"
    );

    let manifest = read(&pkg.join("Cargo.toml"));
    let value: toml::Value = toml::from_str(&manifest)
        .unwrap_or_else(|e| panic!("the emitted manifest is not TOML: {e}\n{manifest}"));
    assert_eq!(value["package"]["edition"].as_str(), Some("2024"));

    // The name is the app's `core` package name, cross-checked against the one
    // the CLI wrote into the cache rather than recomputed here.
    let cache = container(root, "s2");
    assert_eq!(
        package_name(&pkg.join("Cargo.toml")),
        package_name(&cache.join("core/Cargo.toml")),
        "the in-tree package and the cache package must be one package, at two \
         destinations"
    );
    assert!(
        package_name(&pkg.join("Cargo.toml")).ends_with("-core"),
        "the emitted package is not a `core` package"
    );

    // No `[workspace]` table. Not a preference: a nested package carrying one
    // that any member path-depends on fails the whole workspace with `multiple
    // workspace roots found in the same workspace` (#430, closed as not a
    // defect). A generated package with no `[workspace]` is the correct shape.
    assert!(
        value.get("workspace").is_none(),
        "the emitted package must carry no [workspace] table:\n{manifest}"
    );
}

// ===========================================================================
// Scenario 3 — three copies, one value
// ===========================================================================

/// **Scenario 3.** `generated/core/src/lib.rs`, `generated/database.rs` and the
/// cache's `core/src/lib.rs` are byte-identical.
///
/// Extends #335's scenario 29 from two sinks to three. A byte compare rather
/// than a substring check for the reason that one records: the defect it guards
/// was two files that were *mostly* the same and differed only in the durability
/// semantics two generator invocations baked into them. Any assertion weaker
/// than "identical" passes while that bug is present.
#[test]
fn scenario_3_three_copies_one_value() {
    let tmp = project("s3", "\"rust\", \"api\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let cache = container(root, "s3");
    let mirror = read(&root.join("generated/database.rs"));
    let in_tree = read(&root.join("generated/core/src/lib.rs"));
    let cached = read(&cache.join("core/src/lib.rs"));

    assert_eq!(mirror, in_tree, "the mirror and the in-tree package disagree");
    assert_eq!(in_tree, cached, "the in-tree package and the cache disagree");
}

// ===========================================================================
// Scenario 4b — the printed line names the package ForgeDB actually wrote
// ===========================================================================

/// **Scenario 4b.** The printed dep line parses as TOML, its `package` equals
/// the `[package].name` in the emitted manifest, and its `path` points at the
/// emitted directory.
///
/// **Asserted against the manifest, never against a literal.** A literal is
/// exactly how gate 1's line came to be wrong: `forgedb_core = { path = … }`
/// reads perfectly and is a hard cargo error, because cargo matches a path dep's
/// key against the package's own name.
#[test]
fn scenario_4b_the_printed_line_names_the_package_forgedb_wrote() {
    let tmp = project("s4", "\"rust\"", PLACEMENT);
    let root = tmp.path();
    let out = ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let line = printed_dep_line(&out);
    let parsed: toml::Value = toml::from_str(&line)
        .unwrap_or_else(|e| panic!("the printed dep line is not valid TOML: {e}\n{line}"));
    let dep = &parsed["forgedb_core"];

    let manifest = root.join("generated/core/Cargo.toml");
    assert_eq!(
        dep["package"].as_str(),
        Some(package_name(&manifest).as_str()),
        "the printed line renames to a package ForgeDB did not write: {line}"
    );

    let printed_path = dep["path"].as_str().expect("the line carries a path");
    let resolved = root.join(printed_path);
    assert!(
        resolved.join("Cargo.toml").is_file(),
        "the printed path {printed_path} does not hold the emitted manifest"
    );
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(root.join("generated/core")).unwrap(),
    );
}

// ===========================================================================
// Scenario 6 — the path is schema-relative, not CWD-relative
// ===========================================================================

/// **Scenario 6.** One root config, two apps in sibling subdirectories, both
/// generated **from the repo root**: each package lands beside its own schema
/// and neither overwrites the other.
///
/// This is `Governing::output`'s lesson re-applied to the new knob. A shared
/// value read against the CWD makes every app in a project clobber its siblings,
/// and it does so silently — the last generate wins and everything still builds.
#[test]
fn scenario_6_the_placement_is_schema_relative() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"s6\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"generated/core\"\n",
    );
    write(&root.join("a/schema.forge"), SCHEMA);
    write(&root.join("b/schema.forge"), SCHEMA);

    for app in ["a", "b"] {
        ok(
            &forgedb(
                root,
                &["generate", "all", "--force", "--schema", &format!("{app}/schema.forge")],
            ),
            "generate",
        );
    }

    for app in ["a", "b"] {
        let manifest = root.join(app).join("generated/core/Cargo.toml");
        assert!(
            manifest.is_file(),
            "app {app} got no package beside its own schema"
        );
    }
    assert!(
        !root.join("generated/core").exists(),
        "the placement resolved against the CWD, so both apps wrote to one directory"
    );
    assert_ne!(
        package_name(&root.join("a/generated/core/Cargo.toml")),
        package_name(&root.join("b/generated/core/Cargo.toml")),
        "two apps in one project emitted one package name"
    );
}

// ===========================================================================
// Scenario 7 — a rewritten manifest carries the new pins
// ===========================================================================

/// **Scenario 7.** A hand-edited manifest and a hand-edited `src/lib.rs` are both
/// rewritten in full on the next generate.
///
/// This is the property the whole design turns on: #290's floor problem does
/// **not** relocate into user property, because the package is ForgeDB's file
/// and a CLI upgrade's substrate pin reaches an existing project the same way it
/// reaches a cache member. A `write_file` (write-once) path here would make a
/// stale pin survivable, which is the failure this guards.
#[test]
fn scenario_7_a_hand_edit_is_rewritten_in_full() {
    let tmp = project("s7", "\"rust\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "first generate");

    let manifest = root.join("generated/core/Cargo.toml");
    let lib = root.join("generated/core/src/lib.rs");
    let before = read(&manifest);

    // A downgraded substrate pin and a source edit — the two shapes of the same
    // failure.
    write(
        &manifest,
        &before.replace("forgedb-storage = \"0.3\"", "forgedb-storage = \"0.1\""),
    );
    write(&lib, "// I edited ForgeDB's file\n");

    ok(&forgedb(root, &["generate", "all", "--force"]), "second generate");

    assert_eq!(read(&manifest), before, "the hand-edited pin survived");
    assert_ne!(
        read(&lib),
        "// I edited ForgeDB's file\n",
        "the hand-edited source survived"
    );
    assert_eq!(
        read(&lib),
        read(&root.join("generated/database.rs")),
        "the rewritten source is not the database this run generated"
    );
}

// ===========================================================================
// Scenario 10 — in-tree carries no server
// ===========================================================================

/// **Scenario 10.** With `api` declared, the in-tree directory holds
/// `Cargo.toml` + `src/lib.rs` and no `main.rs`, `api.rs` or `[[bin]]`, while the
/// cache still holds its `server/` package.
///
/// Asserted from both sides on purpose: "in-tree has no server" and "the server
/// still exists" are different claims, and a change that deleted the server
/// outright would satisfy only the first.
#[test]
fn scenario_10_in_tree_carries_no_server() {
    let tmp = project("s10", "\"rust\", \"api\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let pkg = root.join("generated/core");
    for absent in ["src/main.rs", "src/api.rs", "main.rs", "api.rs"] {
        assert!(
            !pkg.join(absent).exists(),
            "the in-tree package carries {absent}"
        );
    }
    let manifest = read(&pkg.join("Cargo.toml"));
    assert!(
        !manifest.contains("[[bin]]"),
        "the in-tree package declares a binary:\n{manifest}"
    );

    let cache = container(root, "s10");
    assert!(
        cache.join("server/src/main.rs").is_file(),
        "the cache lost its server package"
    );
    assert!(cache.join("server/src/api.rs").is_file());
}

// ===========================================================================
// Scenario 11b — the utoipa pin agrees with the derive, in-tree
// ===========================================================================

/// **Scenario 11b.** Under `targets = ["all"]`, `generate rust --force` — an
/// invocation that emits no `api.rs` — writes an in-tree manifest that pins
/// `utoipa` **iff** its `src/lib.rs` names it.
///
/// "In-tree carries no `api.rs`" is about the *package*, not the *pin*. When the
/// app declares `api`, `GenConfig::web` is true, the `ToSchema` derive is in
/// `database.rs`, and the manifest must pin utoipa or the consumer's own
/// `cargo build` fails with `E0432: unresolved import 'utoipa'`. Reading the
/// decision as "no api.rs means no utoipa" is the obvious wrong turn, and it
/// produces the failure in the user's build rather than in ours.
#[test]
fn scenario_11b_the_in_tree_utoipa_pin_agrees_with_the_derive() {
    for (tag, targets) in [("s11b-all", "\"all\""), ("s11b-rust", "\"rust\"")] {
        let tmp = project(tag, targets, PLACEMENT);
        let root = tmp.path();
        ok(&forgedb(root, &["generate", "rust", "--force"]), "generate rust");

        let manifest = read(&root.join("generated/core/Cargo.toml"));
        let lib = read(&root.join("generated/core/src/lib.rs"));
        assert_eq!(
            manifest.contains("\nutoipa = "),
            lib.contains("use utoipa::ToSchema;"),
            "the in-tree manifest and its own source disagree about utoipa \
             (targets = {targets}) — the emitted crate does not compile"
        );
    }
}

// ===========================================================================
// Scenario 16a — two apps, two package names
// ===========================================================================

/// **Scenario 16a.** Two schemas in one project, each placing in its own
/// directory, emit two packages with **different** `[package] name`s.
///
/// One name for two packages is a workspace-wide cargo error, so this is the
/// cheap half of the property scenario 16b compiles.
#[test]
fn scenario_16a_two_apps_emit_two_package_names() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"s16a\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"generated/core\"\n",
    );
    write(&root.join("blog/schema.forge"), SCHEMA);
    write(&root.join("shop/schema.forge"), SCHEMA);

    for app in ["blog", "shop"] {
        ok(
            &forgedb(
                root,
                &["generate", "all", "--force", "--schema", &format!("{app}/schema.forge")],
            ),
            "generate",
        );
    }

    assert_ne!(
        package_name(&root.join("blog/generated/core/Cargo.toml")),
        package_name(&root.join("shop/generated/core/Cargo.toml")),
    );
}

// ===========================================================================
// Scenario 9 / 17 — `build` regenerates the package and never compiles it
// ===========================================================================

/// **Scenario 9.** `forgedb build --plan` plans no cargo invocation naming the
/// in-tree directory.
///
/// Class D has **no delivery step**; this is what keeps that true in code rather
/// than in prose. The planned set is exactly the cache packages, and the in-tree
/// package is compiled by the consumer's cargo or by nobody.
///
/// **Scenario 17.** The same run leaves the in-tree package current and
/// byte-identical to the mirror — `build` regenerates before it plans, so a
/// project with the knob set must not have `build` and `generate` emit different
/// project states.
#[test]
fn scenarios_9_and_17_build_regenerates_the_package_but_never_plans_it() {
    let tmp = project("s9", "\"rust\", \"api\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    // Make the in-tree package stale in a way only a regeneration can fix.
    let lib = root.join("generated/core/src/lib.rs");
    write(&lib, "// stale\n");

    let out = ok(&forgedb(root, &["build", "--plan"]), "build --plan");

    // Scenario 17: regenerated, and identical to the mirror.
    assert_eq!(
        read(&lib),
        read(&root.join("generated/database.rs")),
        "`build` left the in-tree package stale"
    );

    // Scenario 9: nothing planned names it. Compared on the CANONICAL path, so a
    // plan printing an absolute path cannot slip past a relative-string check.
    let placement = std::fs::canonicalize(root.join("generated/core")).unwrap();
    let needle = placement.to_string_lossy().to_string();
    let plan_lines: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("cargo") || l.contains("-p "))
        .collect();
    assert!(
        !plan_lines.is_empty(),
        "`build --plan` printed no plan at all:\n{out}"
    );
    for line in &plan_lines {
        assert!(
            !line.contains(&needle) && !line.contains("generated/core"),
            "a planned invocation names the in-tree package: {line}"
        );
    }
}

// ===========================================================================
// Scenario 8 — `--check` compares and writes nothing
// ===========================================================================

/// **Scenario 8.** A stale committed in-tree package makes `generate --check`
/// exit non-zero naming the stale path, and leaves the on-disk bytes
/// **unchanged**; a current one exits 0, also unchanged.
///
/// Both assertions are load-bearing. `--check` is CI's staleness gate for
/// committed generated source, and the in-tree package IS committed source — so
/// a check that skipped it would report a clean tree while the package cargo
/// compiles is a schema behind. And a check that *fixed* the file would be
/// worse than useless in CI: it would pass on every run and gate nothing.
#[test]
fn scenario_8_check_compares_and_writes_nothing() {
    let tmp = project("s8", "\"rust\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let lib = root.join("generated/core/src/lib.rs");
    let manifest = root.join("generated/core/Cargo.toml");

    // Current → exit 0, bytes untouched.
    let before_lib = read(&lib);
    let before_manifest = read(&manifest);
    let out = forgedb(root, &["generate", "all", "--check"]);
    assert!(
        out.status.success(),
        "--check failed on a current tree:\n{}",
        combined(&out)
    );
    assert_eq!(read(&lib), before_lib, "--check rewrote the source");
    assert_eq!(read(&manifest), before_manifest, "--check rewrote the manifest");

    // Stale → non-zero, names the path, bytes STILL untouched.
    write(&lib, "// a schema behind\n");
    let out = forgedb(root, &["generate", "all", "--check"]);
    assert!(
        !out.status.success(),
        "--check passed on a stale in-tree package:\n{}",
        combined(&out)
    );
    let report = combined(&out);
    assert!(
        report.contains("core/src/lib.rs"),
        "--check did not name the stale in-tree path:\n{report}"
    );
    assert_eq!(
        read(&lib),
        "// a schema behind\n",
        "--check repaired the file it was supposed to report"
    );

    // A MISSING package is reported too — the fresh-clone case, where the
    // directory a path dep names does not exist at all.
    std::fs::remove_dir_all(root.join("generated/core")).unwrap();
    let out = forgedb(root, &["generate", "all", "--check"]);
    assert!(
        !out.status.success(),
        "--check passed on a missing in-tree package:\n{}",
        combined(&out)
    );
    assert!(
        !root.join("generated/core").exists(),
        "--check recreated the package it was supposed to report"
    );
}

// ===========================================================================
// Scenario 12 — a placement inside the build cache is refused
// ===========================================================================

/// **Scenario 12.** A `rust_package` resolving inside `$FORGEDB_HOME` exits
/// non-zero naming the cache and why — **and nothing was written**: no in-tree
/// directory, and the mirror was never written either.
///
/// The second half is the [[red-for-the-wrong-reason]] lesson from #345, applied
/// in advance. Mutating the predicate proves the guard *works*; only the
/// "nothing was written" assertion can fail when the guard is *not called*, or
/// is called too late. `cache::assert_not_in_cache` was fully mutation-tested
/// while having executed zero times.
#[test]
fn scenario_12_a_placement_inside_the_cache_is_refused() {
    // `.home` is this fixture's FORGEDB_HOME (see `forgedb`), so this is a
    // placement literally inside the build cache.
    let tmp = project(
        "s12",
        "\"rust\"",
        "\n[placement]\nrust_package = \".home/projects/sneaky/core\"\n",
    );
    let root = tmp.path();

    let out = forgedb(root, &["generate", "all", "--force"]);
    assert!(
        !out.status.success(),
        "a placement inside the build cache was accepted:\n{}",
        combined(&out)
    );

    let report = combined(&out);
    assert!(
        report.contains("build cache"),
        "the refusal does not name the cache:\n{report}"
    );
    assert!(
        report.contains("deleted at any time"),
        "the refusal does not say WHY (C1/C8 — the cache is derived state):\n{report}"
    );
    assert!(
        report.contains("rust_package"),
        "the refusal does not name the key at fault:\n{report}"
    );

    // Nothing was written — this is the half that fails when the guard is not
    // CALLED, or is called after the emitters have already run.
    assert!(
        !root.join(".home/projects/sneaky").exists(),
        "the refused placement was written anyway"
    );
    assert!(
        !root.join("generated/database.rs").exists(),
        "the mirror was written before the placement was refused — the guard \
         runs too late"
    );
}

// ===========================================================================
// The halves that run real cargo (tier 2 — `make test-ignored`)
// ===========================================================================
//
// These build a real consumer workspace against a `[patch.crates-io]` block
// pointing at this checkout. That proves the emitted package **compiles**; it
// proves nothing about **registry resolution**, because a patch table is
// precisely the thing that makes a registry lookup not happen. See this file's
// module doc.

/// This checkout's root, from the test binary's own manifest dir.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The `[patch.crates-io]` block that points every substrate crate at this
/// checkout. Kept in the same shape as `build_cache_compile_test::patch_substrate`
/// — the list has to cover the transitive substrate (`storage` is a facade over
/// `storage-native`/`storage-web`), not just what `core` names directly.
fn patch_block() -> String {
    let mut body = String::from("\n[patch.crates-io]\n");
    for dir in [
        "storage",
        "storage-native",
        "storage-web",
        "types",
        "changefeed",
        "wal",
        "compaction",
        "txn",
        "coordinator",
        "auth",
        "query-params",
    ] {
        let path = repo_root().join("crates").join(dir);
        assert!(path.is_dir(), "no such substrate crate: {}", path.display());
        body.push_str(&format!(
            "forgedb-{dir} = {{ path = {:?} }}\n",
            path.to_string_lossy()
        ));
    }
    body
}

/// Run cargo in `dir` with an explicit `--target-dir` and `CARGO_TARGET_DIR`
/// removed.
///
/// Both are required, not tidy: an ambient env var — **or** a `[build]
/// target-dir` in `$CARGO_HOME/config.toml`, which is machine-wide and needs no
/// env var at all (#292) — would redirect this into the directory the outer
/// `cargo test` holds a lock on, and the test would hang rather than fail.
fn cargo(dir: &Path, target_dir: &Path, args: &[&str]) -> Output {
    let compiles = args
        .first()
        .is_some_and(|a| *a == "build" || *a == "check" || *a == "run");
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.args(args);
    if compiles {
        cmd.arg("--target-dir").arg(target_dir);
    }
    cmd.current_dir(dir)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo runs")
}

/// The `[package] name`s `cargo metadata --no-deps` reports as workspace
/// members.
///
/// Read from cargo's own JSON, never scraped from a path string: a name matched
/// as a substring of a directory goes silently wrong the moment the naming
/// scheme changes (#386).
fn metadata_members(dir: &Path, target_dir: &Path) -> Vec<String> {
    let out = cargo(dir, target_dir, &["metadata", "--no-deps", "--format-version", "1"]);
    assert!(
        out.status.success(),
        "`cargo metadata` rejects the consumer workspace:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata emits JSON");
    let mut names: Vec<String> = json["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| p["name"].as_str().expect("name").to_string())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Scenario 5 ★ — the printed line builds, and the database runs
// ---------------------------------------------------------------------------

/// **Scenario 5 (★).** In a real cargo workspace outside this checkout, pasting
/// the printed dep line **verbatim** into a crate's `[dependencies]` makes the
/// generated package a workspace member although `members` was never edited,
/// `cargo build` succeeds, and a `main` that opens a `Database`, inserts a row
/// and reads it back runs and exits 0.
///
/// This is the scenario the whole feature reduces to, and no string-level test
/// can reach any part of it: the two halves this bug class produces — a manifest
/// and a source file — are each individually well-formed and fail only when they
/// disagree.
///
/// The consumer here is the workspace **root package**, with a second member
/// beside it. That shape is deliberate and is the one place the printed line is
/// paste-able unchanged: `path` is written relative to the directory `generate`
/// ran in, so a consumer crate in a *subdirectory* must re-base it. The CLI says
/// so when it prints the line.
#[test]
#[ignore = "compiles a real consumer workspace; run with --ignored"]
fn scenario_5_the_printed_line_builds_and_the_database_runs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ws = tmp.path();

    write(
        &ws.join("schema.forge"),
        "Note {\n  id: +uuid\n  title: string\n}\n",
    );
    write(
        &ws.join("forgedb.toml"),
        "[project]\nid = \"s5\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"generated/core\"\n",
    );

    let out = ok(&forgedb(ws, &["generate", "all", "--force"]), "generate all");
    let dep_line = printed_dep_line(&out);

    // A workspace whose root is also a package, plus one unrelated member. The
    // `members` array names only `helper` — the generated package joins because
    // it is a path dependency inside the workspace directory, and that is the
    // mechanism the whole design rests on.
    write(
        &ws.join("Cargo.toml"),
        &format!(
            "[workspace]\nmembers = [\"helper\"]\n\n\
             [package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\n{dep_line}\n{}",
            patch_block()
        ),
    );
    write(
        &ws.join("helper/Cargo.toml"),
        "[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    );
    write(&ws.join("helper/src/lib.rs"), "pub fn helper() {}\n");
    write(
        &ws.join("src/main.rs"),
        r#"
use forgedb_core::forgedb_types::Uuid;
use forgedb_core::{Database, Note};

fn main() {
    let dir = std::env::temp_dir().join(format!("forgedb-338-s5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut db = Database::open_at(dir.clone());
    let id = db
        .create_note(Note { id: Uuid::nil(), title: "hello".to_string() })
        .expect("insert");
    db.commit().expect("commit");

    let got = db.note.get(id).expect("the row reads back");
    assert_eq!(got.title, "hello");

    let _ = std::fs::remove_dir_all(&dir);
    println!("forgedb-338-ok {id}");
}
"#,
    );

    let target_dir = tmp.path().join(".cargo-target");

    // The generated package is a member although `members` names only `helper`.
    let members = metadata_members(ws, &target_dir);
    let core = package_name(&ws.join("generated/core/Cargo.toml"));
    assert!(
        members.contains(&core),
        "the generated package did not join the workspace: {members:?} (wanted {core})"
    );
    assert!(members.contains(&"consumer".to_string()));
    assert!(members.contains(&"helper".to_string()));
    // Scoped to the `members` ARRAY, read back through TOML — the manifest DOES
    // name the core package, in the dep line, and that is the point. A whole-file
    // `contains` would be satisfied by the very thing under test.
    let root_manifest: toml::Value =
        toml::from_str(&read(&ws.join("Cargo.toml"))).expect("the consumer root parses");
    let listed: Vec<&str> = root_manifest["workspace"]["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|m| m.as_str().expect("a member is a string"))
        .collect();
    assert_eq!(
        listed,
        vec!["helper"],
        "the test edited `members` — the path-dep auto-join is what is under test"
    );

    let build = cargo(ws, &target_dir, &["build", "-p", "consumer"]);
    assert!(
        build.status.success(),
        "the consumer workspace does not build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    // `build`, then RUN. A crate that compiles and cannot open a database is
    // exactly the failure a compile-only check reports as green.
    let run = cargo(ws, &target_dir, &["run", "-p", "consumer"]);
    assert!(
        run.status.success(),
        "the consumer binary did not run:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("forgedb-338-ok"),
        "the binary exited 0 without reaching its own sentinel:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

// ---------------------------------------------------------------------------
// Scenario 11c — the narrowing invocation's package compiles
// ---------------------------------------------------------------------------

/// **Scenario 11c.** The in-tree package produced by `generate rust --force`
/// under `targets = ["all"]` **compiles**.
///
/// This is the invocation that narrows: it emits no `api.rs`, but the app
/// declares `api`, so `GenConfig::web` is true and the source carries the
/// `ToSchema` derives. A manifest computed from "did this command emit an api"
/// would omit the utoipa pin and the crate would fail with
/// `error[E0432]: unresolved import 'utoipa'` — in the user's own build.
/// 11b asserts the pairing as strings; only this one compiles it.
#[test]
#[ignore = "compiles a generated package; run with --ignored"]
fn scenario_11c_the_narrowing_invocations_package_compiles() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ws = tmp.path();

    write(&ws.join("schema.forge"), SCHEMA);
    write(
        &ws.join("forgedb.toml"),
        "[project]\nid = \"s11c\"\n\n[generate]\ntargets = [\"all\"]\n[placement]\nrust_package = \"generated/core\"\n",
    );

    let out = ok(&forgedb(ws, &["generate", "rust", "--force"]), "generate rust");
    let dep_line = printed_dep_line(&out);

    write(
        &ws.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\n{dep_line}\n{}",
            patch_block()
        ),
    );
    write(&ws.join("src/lib.rs"), "pub use forgedb_core::*;\n");

    let target_dir = tmp.path().join(".cargo-target");
    let build = cargo(ws, &target_dir, &["build"]);
    assert!(
        build.status.success(),
        "the package a narrowing generate wrote does not compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

// ---------------------------------------------------------------------------
// Scenario 16b — two apps, one consumer workspace
// ---------------------------------------------------------------------------

/// **Scenario 16b.** A real workspace depending on two ForgeDB apps: `cargo
/// metadata` lists both generated packages as members and `cargo build`
/// succeeds.
///
/// Each app is consumed by its own member crate. That is not incidental — the
/// printed dep line uses the key `forgedb_core` for every app, so a **single**
/// crate depending on two ForgeDB apps must rename one of the two keys itself.
/// One member per app is the shape that needs no such edit.
#[test]
#[ignore = "compiles a real consumer workspace; run with --ignored"]
fn scenario_16b_two_apps_build_in_one_workspace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ws = tmp.path();

    write(
        &ws.join("forgedb.toml"),
        "[project]\nid = \"s16b\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"core\"\n",
    );

    let mut dep_lines = Vec::new();
    for app in ["blog", "shop"] {
        write(
            &ws.join(app).join("schema.forge"),
            "Note {\n  id: +uuid\n  title: string\n}\n",
        );
        let out = ok(
            &forgedb(
                ws,
                &["generate", "all", "--force", "--schema", &format!("{app}/schema.forge")],
            ),
            "generate",
        );
        dep_lines.push(printed_dep_line(&out));
    }

    write(
        &ws.join("Cargo.toml"),
        &format!(
            "[workspace]\nmembers = [\"blog/app\", \"shop/app\"]\nresolver = \"3\"\n{}",
            patch_block()
        ),
    );
    for (app, line) in ["blog", "shop"].iter().zip(&dep_lines) {
        // Each member sits one level below the directory `generate` ran in, so
        // the printed path is re-based here — the same edit a real consumer
        // makes, and the reason the CLI says what the path is relative to.
        let rebased = line.replace(
            &format!("path = \"{app}/core\""),
            "path = \"../core\"",
        );
        assert_ne!(&rebased, line, "the printed path was not the one re-based: {line}");
        write(
            &ws.join(app).join("app/Cargo.toml"),
            &format!(
                "[package]\nname = \"{app}-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
                 [dependencies]\n{rebased}\n"
            ),
        );
        write(
            &ws.join(app).join("app/src/lib.rs"),
            "pub fn open(p: std::path::PathBuf) -> forgedb_core::Database {\n    \
             forgedb_core::Database::open_at(p)\n}\n",
        );
    }

    let target_dir = tmp.path().join(".cargo-target");
    let members = metadata_members(ws, &target_dir);
    for app in ["blog", "shop"] {
        let core = package_name(&ws.join(app).join("core/Cargo.toml"));
        assert!(
            members.contains(&core),
            "{app}'s generated package did not join the workspace: {members:?}"
        );
    }

    let build = cargo(ws, &target_dir, &["build"]);
    assert!(
        build.status.success(),
        "a workspace holding two ForgeDB apps does not build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
}
