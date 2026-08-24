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
            "[project]\nname = \"{tag}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n{placement}"
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
        "[project]\nname = \"s6\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"generated/core\"\n",
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
        "[project]\nname = \"s16a\"\n\n[generate]\ntargets = [\"rust\"]\n[placement]\nrust_package = \"generated/core\"\n",
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
