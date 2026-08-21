//! Which project is this? — the #333 scenarios (gate #341's table).
//!
//! Two styles, chosen per scenario rather than by preference:
//!
//! * **In-process**, against `forgedb::project`, wherever the answer is a pure
//!   function of a directory tree.  `Chain::walk` takes an explicit absolute
//!   path, so these carry no dependency on the CWD and can run in parallel.
//! * **Subprocess**, via `CARGO_BIN_EXE_forgedb` with an explicit `current_dir`,
//!   wherever the scenario is *about* the invocation (which directory you ran
//!   from) or touches the claim ledger.  Every subprocess here sets
//!   `FORGEDB_HOME` to a tempdir — one that does not would write claims into the
//!   developer's real `~/.forgedb` and pass while doing it.
//!
//! **Every fixture puts a `.git` marker at its root.** The walk's stop boundary
//! is a repository root, `$HOME`, or the filesystem root; a tempdir under `/tmp`
//! is none of those, so without the marker these fixtures would walk to `/` and
//! could pick up a stray config on the machine running them (gate #341, gotcha
//! 7 — the boundary only fails on a real `$HOME`, so it has to be constructed
//! deliberately rather than assumed).

use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::project::{self, Chain, IdSource};

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");

/// A minimal schema that parses and generates.
const SCHEMA: &str = "Note {\n  id: +uuid\n  body: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A fixture root with the repository marker that bounds the walk.
fn repo_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    root
}

/// Run the CLI with an isolated cache home, from an explicit directory.
fn run(cwd: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .output()
        .expect("forgedb binary runs")
}

/// The one `ffi` package this project's cache holds, if any.
///
/// **The discriminator for "did `all` reach the opt-in targets" lives in the
/// CACHE, not in the output directory** (#335 step 7): `generate` stopped
/// writing `ffi/`, `napi/`, `pyo3/` and `replica/` into the user's tree and
/// emits them as members of the project's cargo workspace instead. Asserting on
/// `generated/ffi` after that flip is asserting on a directory nothing writes
/// any more — it would fail whatever the target filter did, which is the same
/// as not testing the filter.
///
/// Found by scan rather than by joining a hash: the app hash is FNV-1a over the
/// project-relative schema path, and recomputing it here would be a second
/// derivation of something the CLI already settled.
fn cache_ffi_package(home: &Path) -> Option<PathBuf> {
    let projects = home.join("projects");
    for project in std::fs::read_dir(&projects).ok()?.flatten() {
        let apps = project.path().join("apps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for app in entries.flatten() {
            let ffi = app.path().join("ffi");
            if ffi.join("Cargo.toml").is_file() {
                return Some(ffi);
            }
        }
    }
    None
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The `Project: <id>` line `-v` emits, which is how a subprocess observes an
/// identity without the cache dir existing yet.
fn project_line(out: &std::process::Output) -> String {
    combined(out)
        .lines()
        .find(|l| l.contains("Project:"))
        .map(|l| {
            l.split("Project:").nth(1).unwrap().trim().to_string()
        })
        .unwrap_or_else(|| panic!("no Project: line in output:\n{}", combined(out)))
}

// ---------------------------------------------------------------------------
// 1–4: one walk, two answers
// ---------------------------------------------------------------------------

/// Knobs come from the schema's nearest ancestor config, not from the CWD.
///
/// This is the scenario that fails if `Chain::walk` starts where the user is
/// standing instead of where the schema is.
#[test]
fn scenario_1_knobs_come_from_the_schemas_nearest_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nname = \"mono\"\n[storage]\nfsync = \"always\"\n",
    );
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nisolated = false\n[storage]\nfsync = \"never\"\n",
    );
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    let nearest = chain.nearest().expect("a config was found");

    assert_eq!(nearest.dir, root.join("apps/api"));
    assert_eq!(nearest.config.storage.fsync.as_deref(), Some("never"));
}

/// Identity comes from the OUTERMOST config when no `isolated` intervenes, so
/// two apps under one root are one project.
///
/// Deliberately asserted alongside scenario 1's tree: knobs said `apps/api`,
/// identity says the root. Code that resolves "the project config" once and uses
/// it for both compiles, runs, and mis-keys the cache silently — an assertion on
/// only one of the two answers cannot see that.
#[test]
fn scenario_2_identity_comes_from_the_outermost_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");
    for app in ["api", "web"] {
        write(
            &root.join(format!("apps/{app}/forgedb.toml")),
            "[project]\nisolated = false\n",
        );
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
    }

    let api = project::identify(&Chain::walk(&root.join("apps/api")).unwrap()).unwrap();
    let web = project::identify(&Chain::walk(&root.join("apps/web")).unwrap()).unwrap();

    assert_eq!(api.name, "mono");
    assert_eq!(api, web, "two apps under one root are one project");
    assert_eq!(api.root, root);

    // …while the knobs still came from each app's own config.
    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    assert_eq!(chain.nearest().unwrap().dir, root.join("apps/api"));
    assert_eq!(chain.project_root().unwrap().dir, root);
}

/// `isolated = true` on an intermediate config stops the identity walk there,
/// while leaving the knob rule untouched.
#[test]
fn scenario_3_isolated_stops_the_walk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nname = \"api\"\nisolated = true\n[storage]\nfsync = \"never\"\n",
    );
    write(&root.join("apps/api/schema.forge"), SCHEMA);
    write(
        &root.join("apps/web/forgedb.toml"),
        "[project]\nisolated = false\n",
    );
    write(&root.join("apps/web/schema.forge"), SCHEMA);

    let api = project::identify(&Chain::walk(&root.join("apps/api")).unwrap()).unwrap();
    let web = project::identify(&Chain::walk(&root.join("apps/web")).unwrap()).unwrap();

    assert_eq!(api.name, "api");
    assert_eq!(api.root, root.join("apps/api"));
    assert_eq!(web.name, "mono", "its sibling still belongs to the monorepo");

    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    assert_eq!(
        chain.nearest().unwrap().config.storage.fsync.as_deref(),
        Some("never"),
        "isolated governs identity only — knobs are still nearest-wins"
    );
}

/// An omitted `isolated` means isolated, so a config chain written before #333
/// does not silently regroup on upgrade.
///
/// This is the scenario the `bool` default would get backwards: read naturally,
/// absent means "not isolated" — grouped — and every app under a pre-existing
/// root config would collapse into one project and one lockfile.
#[test]
fn scenario_4_omitted_isolated_means_isolated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");
    // Neither app config mentions `isolated`, exactly as a pre-#333 tree would.
    write(&root.join("apps/api/forgedb.toml"), "[storage]\nfsync = \"never\"\n");
    write(&root.join("apps/api/schema.forge"), SCHEMA);
    write(&root.join("apps/web/forgedb.toml"), "[storage]\nfsync = \"never\"\n");
    write(&root.join("apps/web/schema.forge"), SCHEMA);

    let api = project::identify(&Chain::walk(&root.join("apps/api")).unwrap()).unwrap();
    let web = project::identify(&Chain::walk(&root.join("apps/web")).unwrap()).unwrap();

    assert_eq!(api.root, root.join("apps/api"));
    assert_eq!(web.root, root.join("apps/web"));
    assert_ne!(api.name, web.name, "no grouping happened on upgrade");
}

// ---------------------------------------------------------------------------
// 5–8: identity resolution
// ---------------------------------------------------------------------------

/// `[project].name` at a config that is not the project root is a positioned
/// error naming the key and its line.
#[test]
fn scenario_5_name_at_a_non_root_config_is_a_positioned_error() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        // `name` deliberately on line 4, so a hard-coded 1 or 2 fails.
        "[project]\nisolated = false\nversion = \"0.1.0\"\nname = \"api\"\n",
    );

    let err = project::identify(&Chain::walk(&root.join("apps/api")).unwrap())
        .expect_err("a nested config naming a project is a contradiction");
    let msg = err.to_string();

    assert!(msg.contains("apps/api/forgedb.toml:4:1"), "positioned at the key: {msg}");
    assert!(msg.contains("[project].name"), "names the key: {msg}");
    assert!(msg.contains("isolated = true"), "offers the remedy: {msg}");
}

/// An explicit `[project].name` at the root wins over a detectable manifest.
#[test]
fn scenario_6_explicit_name_wins_over_a_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"chosen\"\n");
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"detected\"\nversion = \"0.1.0\"\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let id = project::identify(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(id.name, "chosen");
    assert_eq!(id.source, IdSource::Explicit);
}

/// Exactly one detectable manifest is picked up, and its name becomes the id.
/// More than one is refused rather than guessed.
#[test]
fn scenario_7_exactly_one_manifest_is_auto_picked() {
    // One manifest → adopted.
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nisolated = true\n");
    write(
        &root.join("package.json"),
        "{ \"name\": \"storefront\", \"version\": \"1.0.0\" }",
    );
    let id = project::identify(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(id.name, "storefront");
    assert_eq!(id.source, IdSource::Manifest("package.json"));

    // Two → an error that names both and points at the remedy. A repo with a
    // Cargo.toml and a package.json at its root is common, so this path is
    // routinely reached and must not silently prefer one.
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );
    let err = project::identify(&Chain::walk(&root).unwrap()).expect_err("ambiguous");
    let msg = err.to_string();
    assert!(msg.contains("storefront") && msg.contains("backend"), "{msg}");
    assert!(msg.contains("[project].name"), "{msg}");

    // A workspace-only Cargo.toml names nothing, so it does not make the answer
    // ambiguous — reporting no name is the correct reading of it.
    write(&root.join("Cargo.toml"), "[workspace]\nmembers = []\n");
    let id = project::identify(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(id.name, "storefront");
}

/// The path-hash fallback is stable across runs, differs between two roots, and
/// does not change with the CWD.
///
/// The last clause is the one that matters: epic #332 originally said the
/// fallback hashes the absolute **CWD** path, which would give the same project
/// a different id — and a cold build cache — depending on where the user was
/// standing.
#[test]
fn scenario_8_path_hash_fallback_is_stable_and_cwd_independent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("a/forgedb.toml"), "[project]\nisolated = true\n");
    write(&root.join("a/schema.forge"), SCHEMA);
    write(&root.join("b/forgedb.toml"), "[project]\nisolated = true\n");

    let first = project::identify(&Chain::walk(&root.join("a")).unwrap()).unwrap();
    let again = project::identify(&Chain::walk(&root.join("a")).unwrap()).unwrap();
    let other = project::identify(&Chain::walk(&root.join("b")).unwrap()).unwrap();

    assert_eq!(first.source, IdSource::PathHash);
    assert_eq!(first.name, again.name, "stable across runs");
    assert_ne!(first.name, other.name, "distinct roots get distinct ids");

    // …and stable across invocation directories, which only a subprocess with a
    // real CWD can show.
    let home = tempfile::tempdir().unwrap();
    let schema = root.join("a/schema.forge");
    let from_root = run(
        &root,
        home.path(),
        &["-v", "generate", "rust", "--schema", schema.to_str().unwrap(), "--output", root.join("out1").to_str().unwrap()],
    );
    let from_app = run(
        &root.join("a"),
        home.path(),
        &["-v", "generate", "rust", "--schema", schema.to_str().unwrap(), "--output", root.join("out2").to_str().unwrap()],
    );
    assert_eq!(project_line(&from_root), project_line(&from_app));
}

/// The walk stops at the boundary: a config above a repository root is not an
/// ancestor.
#[test]
fn scenario_9_the_walk_stops_at_the_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().canonicalize().unwrap();
    // A config ABOVE the repo — the shape of a stray `~/forgedb.toml`, which
    // would otherwise capture every project beneath it.
    write(&outside.join("forgedb.toml"), "[project]\nname = \"captured\"\n");

    let root = outside.join("repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nname = \"mine\"\n");
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    assert_eq!(chain.links().len(), 1, "only the repo's own config");
    assert_eq!(project::identify(&chain).unwrap().name, "mine");
}

// ---------------------------------------------------------------------------
// 10–12, 16: strict parsing
// ---------------------------------------------------------------------------

/// The scaffold parses strictly against its own CLI.
///
/// `init` and `ForgeConfig` are two files that must agree on the set of tables,
/// and nothing else notices when they drift: an unknown table used to be ignored,
/// so the scaffold kept working while being wrong.
#[test]
fn scenario_10_the_scaffold_parses_strictly() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    let home = tempfile::tempdir().unwrap();

    let out = run(&root, home.path(), &["init", "demo"]);
    assert!(out.status.success(), "init failed:\n{}", combined(&out));

    let emitted = root.join("demo/forgedb.toml");
    let content = std::fs::read_to_string(&emitted).unwrap();
    forgedb::config::parse_config(&content, &emitted)
        .expect("the config `forgedb init` writes must parse with the real loader");

    assert!(
        content.contains("isolated = true"),
        "init records grouping explicitly:\n{content}"
    );
    assert!(
        !content.contains("\nschema = "),
        "the removed [generate].schema key must not be scaffolded:\n{content}"
    );
}

/// An unknown table is an error naming the table and its line.
#[test]
fn scenario_11_an_unknown_table_errors() {
    let err = forgedb::config::parse_config(
        "[project]\nname = \"x\"\n\n[projekt]\nname = \"y\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("a misspelled table must not be ignored");
    let msg = err.to_string();
    assert!(msg.contains("line 4"), "positions the table: {msg}");
    assert!(msg.contains("projekt"), "names the table: {msg}");
}

/// An unknown key inside a known table is an error naming the key and its line.
///
/// This is the case the old forward-compatibility argument got backwards: a knob
/// the CLI does not know reads as applied and is not, so the failure is invisible
/// and surfaces as wrong generated bytes.
#[test]
fn scenario_12_an_unknown_key_errors() {
    let err = forgedb::config::parse_config(
        "[storage]\nfsync = \"never\"\nfsinc = \"always\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("a misspelled key must not be ignored");
    let msg = err.to_string();
    assert!(msg.contains("line 3"), "positions the key: {msg}");
    assert!(msg.contains("fsinc"), "names the key: {msg}");
}

/// A config setting the removed `[generate].schema` fails with the removal
/// diagnostic, not a generic unknown-field error.
///
/// `unknown field 'schema'` would be true and useless: the user cannot tell
/// whether they misspelled something or whether the key is gone.
#[test]
fn scenario_16_removed_generate_schema_has_its_own_diagnostic() {
    let err = forgedb::config::parse_config(
        "[project]\nname = \"x\"\n\n[generate]\noutput = \"./generated\"\nschema = \"schema.forge\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("the removed key must be rejected");
    let msg = err.to_string();

    assert!(msg.contains("was removed"), "says it was removed: {msg}");
    assert!(msg.contains("--schema"), "names the replacement: {msg}");
    assert!(msg.contains(":6:"), "positions the key: {msg}");
    assert!(
        !msg.contains("unknown field"),
        "must not fall through to the generic diagnostic: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 13–14: the ledger detects, the config records
// ---------------------------------------------------------------------------

/// A second root claiming a taken id is refused, and non-interactively it exits
/// non-zero rather than picking a name of its own.
#[test]
fn scenario_13_a_taken_id_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("forgedb.toml"), "[project]\nname = \"clash\"\n");
        write(&root.join("schema.forge"), SCHEMA);
    }

    let first = run(
        &base.join("one"),
        home.path(),
        &["generate", "rust", "--output", base.join("one/generated").to_str().unwrap()],
    );
    assert!(first.status.success(), "first claim succeeds:\n{}", combined(&first));

    let second = run(
        &base.join("two"),
        home.path(),
        &["generate", "rust", "--output", base.join("two/generated").to_str().unwrap()],
    );
    assert!(!second.status.success(), "a taken id must be refused");
    let msg = combined(&second);
    assert!(msg.contains("already claimed"), "{msg}");
    assert!(msg.contains("[project].name"), "names the remedy: {msg}");
}

/// A resolved collision survives a cache wipe.
///
/// This is the C1 guard, and the scenario that fails if anyone later moves the
/// resolution into the ledger: the ledger is derived state that GC may delete at
/// any time, so a resolution recorded there would resurrect the collision — as a
/// silent merge of two projects rather than as an error.
#[test]
fn scenario_14_a_resolved_collision_survives_a_cache_wipe() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("schema.forge"), SCHEMA);
    }
    write(&base.join("one/forgedb.toml"), "[project]\nname = \"clash\"\n");
    // The resolution: the second project picks a different name, in ITS OWN
    // config — not in the ledger.
    write(&base.join("two/forgedb.toml"), "[project]\nname = \"resolved\"\n");

    let regenerate = |side: &str| {
        run(
            &base.join(side),
            home.path(),
            &["generate", "rust", "--force", "--output", base.join(side).join("generated").to_str().unwrap()],
        )
    };
    assert!(regenerate("one").status.success());
    assert!(regenerate("two").status.success());

    // Wipe the whole cache, ledger included.
    std::fs::remove_dir_all(home.path()).unwrap();
    std::fs::create_dir_all(home.path()).unwrap();

    // Re-resolving in the opposite order must still not collide: the names came
    // back from the configs, which the wipe did not touch.
    let two = regenerate("two");
    assert!(two.status.success(), "the resolution survived the wipe:\n{}", combined(&two));
    let after = regenerate("one");
    assert!(after.status.success(), "…and did not merely swap who wins:\n{}", combined(&after));
}

// ---------------------------------------------------------------------------
// 15, 17: what the invocation observes
// ---------------------------------------------------------------------------

/// `generate` and `build` resolve the same project id and the same knobs from
/// the same schema.
///
/// The standing guard on #361's fix, extended to identity: the two commands
/// resolving differently is individually valid on both sides and wrong only
/// together, so nothing but a paired assertion can see it.
#[test]
fn scenario_15_generate_and_build_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nisolated = false\n[storage]\nfsync = \"never\"\n",
    );
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    // Invoked from the repo root against a nested schema — the case where a
    // CWD-based resolution and a schema-based one disagree.
    let args = ["-v", "--schema", "apps/api/schema.forge"];
    let generate = run(&root, home.path(), &["-v", "generate", "rust", "--schema", "apps/api/schema.forge", "--output", root.join("out").to_str().unwrap()]);
    let build = run(&root, home.path(), &["build", &args[0], "--schema", "apps/api/schema.forge"]);

    assert_eq!(project_line(&generate), "mono (from [project].name)");
    assert_eq!(
        project_line(&build),
        project_line(&generate),
        "build must resolve what generate resolved"
    );
}

/// A shared root config's relative `output` is a per-app pattern, not one shared
/// directory two apps interleave their generated code into.
#[test]
fn scenario_17_output_is_schema_relative() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        // `targets` is required as of #335 §12 — a config that declares part of
        // `[generate]` may not leave the most consequential key to be guessed.
        "[project]\nname = \"mono\"\n\n[generate]\noutput = \"generated\"\ntargets = [\"all\"]\n",
    );
    for app in ["api", "web"] {
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
        let out = run(
            &root,
            home.path(),
            &["generate", "rust", "--schema", &format!("apps/{app}/schema.forge")],
        );
        assert!(out.status.success(), "generate {app} failed:\n{}", combined(&out));
    }

    assert!(
        root.join("apps/api/generated").is_dir(),
        "each app gets its own output directory"
    );
    assert!(root.join("apps/web/generated").is_dir());
    assert!(
        !root.join("generated").exists(),
        "…and nothing lands in one shared directory beside the config"
    );
}

/// `init` inside an existing project scaffolds a config that actually resolves.
///
/// Not in gate #341's table, and the sharpest gap in it: the scaffold writes
/// `[project].name` unconditionally, while §6 makes a name at a non-root config
/// an error. Scaffolding into a monorepo would therefore emit a config that fails
/// on the very next `generate` — `init` succeeds, so nothing looks wrong until
/// the user runs the next command.
#[test]
fn init_inside_a_project_scaffolds_a_config_that_resolves() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");

    let out = run(&root, home.path(), &["init", "api"]);
    assert!(out.status.success(), "init failed:\n{}", combined(&out));
    assert!(
        combined(&out).contains("mono"),
        "init reports the project it joined:\n{}",
        combined(&out)
    );

    let emitted = std::fs::read_to_string(root.join("api/forgedb.toml")).unwrap();
    assert!(emitted.contains("isolated = false"), "joins the enclosing project:\n{emitted}");
    assert!(
        !emitted.contains("\nname = "),
        "a joining config must NOT declare a name:\n{emitted}"
    );

    // The real assertion: the scaffolded tree resolves rather than erroring.
    let id = project::identify(&Chain::walk(&root.join("api")).unwrap()).unwrap();
    assert_eq!(id.name, "mono");
}

/// …and standing alone inside a project does name itself.
#[test]
fn init_isolated_inside_a_project_names_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"mono\"\n");

    let out = run(&root, home.path(), &["init", "api", "--isolated"]);
    assert!(out.status.success(), "init failed:\n{}", combined(&out));

    let emitted = std::fs::read_to_string(root.join("api/forgedb.toml")).unwrap();
    assert!(emitted.contains("isolated = true"), "{emitted}");
    assert!(emitted.contains("name = \"api\""), "{emitted}");

    let id = project::identify(&Chain::walk(&root.join("api")).unwrap()).unwrap();
    assert_eq!(id.name, "api");
}

/// C12 at the point the name is chosen: `init` refuses a name another root holds,
/// rather than letting the scaffold succeed and the first `generate` fail.
#[test]
fn init_refuses_a_project_name_another_root_holds() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    let first = base.join("first");
    std::fs::create_dir_all(first.join(".git")).unwrap();
    write(&first.join("forgedb.toml"), "[project]\nname = \"taken\"\n");
    write(&first.join("schema.forge"), SCHEMA);
    let claimed = run(
        &first,
        home.path(),
        &["generate", "rust", "--output", first.join("generated").to_str().unwrap()],
    );
    assert!(claimed.status.success(), "{}", combined(&claimed));

    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(elsewhere.join(".git")).unwrap();
    let out = run(&elsewhere, home.path(), &["init", "taken"]);
    assert!(!out.status.success(), "a taken name must be refused at init");
    assert!(combined(&out).contains("already claimed"), "{}", combined(&out));
}

// ---------------------------------------------------------------------------
// Beyond the plan's table: a hole the scenarios above do not cover
// ---------------------------------------------------------------------------

/// A project name is used verbatim as a directory name under `~/.forgedb`, so a
/// name carrying a path separator would escape the cache rather than key it.
///
/// Not in gate #341's scenario table — it comes from the id being *spent* as a
/// path in #334, which the identity design does not itself say. Left unguarded,
/// `name = "../../etc"` is accepted here and lands in the cache layout.
#[test]
fn a_project_name_that_is_a_path_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);

    for hostile in ["../escape", "a/b", ""] {
        write(
            &root.join("forgedb.toml"),
            &format!("[project]\nname = \"{hostile}\"\n"),
        );
        let err = project::identify(&Chain::walk(&root).unwrap())
            .unwrap_err_or_else_name(hostile);
        assert!(err.contains("project name"), "{hostile}: {err}");
    }
}

/// Small helper so the loop above reads as one assertion per hostile name.
trait NameErr {
    fn unwrap_err_or_else_name(self, name: &str) -> String;
}
impl NameErr for Result<project::ProjectId, forgedb::CliError> {
    fn unwrap_err_or_else_name(self, name: &str) -> String {
        match self {
            Err(e) => e.to_string(),
            Ok(id) => panic!("{name:?} was accepted as a project id: {id:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// #335 step 4 — `[generate].targets` is required, and speaks the CLI vocabulary
//
// Prefixed `s335_` because the scenario numbers above belong to #344/#333's
// plan; these are #347's and the two sets overlap.
// ---------------------------------------------------------------------------

/// **Scenario 16.** A `[generate]` table that omits `targets` is a positioned
/// error naming `["all"]`.
///
/// The refused case is deliberately narrow: a config that declares *part* of
/// `[generate]` and leaves the most consequential key to be guessed. A project
/// with no config file, or with no `[generate]` table, takes the built-in
/// `["all"]` — see the two tests below.
#[test]
fn s335_16_absent_targets_is_a_positioned_error() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nname = \"needs-targets\"\n\n[generate]\noutput = \"generated\"\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "rust"]);
    assert!(!out.status.success(), "should have refused:\n{}", combined(&out));

    let msg = combined(&out);
    assert!(msg.contains("`[generate].targets` is required"), "{msg}");
    assert!(msg.contains("forgedb.toml:4:1"), "not positioned at the table: {msg}");
    assert!(msg.contains("targets = [\"all\"]"), "does not name the remedy: {msg}");
}

/// A project with **no config file at all** keeps working, and the built-in
/// default is a *stated* `["all"]` rather than an absence.
///
/// **This asserts through `generate all`, and that is load-bearing.** A
/// single-target invocation such as `generate rust` never consults
/// `config_targets` at all (#335 §12) — so written that way this test passes
/// whatever the default is, including `None`. Verified by mutation: nulling the
/// default left the `generate rust` form green. `ffi` is the discriminator
/// because it is reachable only by the filter naming it — and it is looked for
/// in the CACHE, per [`cache_ffi_package`].
#[test]
fn s335_16_no_config_file_still_generates() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "no config should be fine:\n{}", combined(&out));
    assert!(root.join("generated/database.rs").is_file());
    assert!(
        cache_ffi_package(home.path()).is_some(),
        "the built-in default did not declare `all`:\n{}",
        combined(&out)
    );
}

/// A config file with no `[generate]` table at all has declared nothing about
/// generation, so it takes the same built-in default — asserted the same way,
/// and for the same reason.
#[test]
fn s335_16_a_config_without_a_generate_table_still_generates() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nname = \"no-gen-table\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        cache_ffi_package(home.path()).is_some(),
        "an absent [generate] table did not take the built-in default:\n{}",
        combined(&out)
    );
}

/// **Scenario 17.** An unknown value is an error naming the legal set.
///
/// Today `targets = ["napi"]` emits **nothing at all** and reports nothing: the
/// filter is present, so every real target is disabled and no error is raised.
/// `napi` is chosen deliberately — it is an *internal* name that was never a
/// legal config value, which is exactly the trap.
#[test]
fn s335_17_an_unknown_target_value_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nname = \"bad-target\"\n\n[generate]\ntargets = [\"napi\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "rust"]);
    assert!(!out.status.success(), "should have refused:\n{}", combined(&out));

    let msg = combined(&out);
    assert!(msg.contains("Unknown `[generate].targets` value `napi`"), "{msg}");
    assert!(msg.contains("node-runtime"), "does not name the replacement: {msg}");
    assert!(
        msg.contains("generate node --runtime"),
        "does not name the CLI equivalent: {msg}"
    );
}

/// **Scenario 19.** A retired spelling still works and **warns**, naming its
/// replacement. Silence here is how the two vocabularies drifted apart.
#[test]
fn s335_19_a_deprecated_target_spelling_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nname = \"deprecated\"\n\n[generate]\ntargets = [\"typescript\", \"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    let msg = combined(&out);
    assert!(msg.contains("node-sdk"), "the warning does not name the replacement: {msg}");

    // ...and it still means what it always meant.
    assert!(root.join("generated/types.ts").is_file(), "the TS output was not emitted");
}

/// **Scenario 18.** `["all"]` genuinely means all — including the targets that
/// were opt-in and therefore unreachable from `generate all` no matter what.
///
/// `ffi` is the discriminator: it has always been gated on the filter *naming*
/// it, so under the old "absent means everything" reading a default project
/// could never emit it.
#[test]
fn s335_18_all_reaches_the_opt_in_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nname = \"reach-all\"\n\n[generate]\ntargets = [\"all\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    // Always-on, as before.
    assert!(root.join("generated/database.rs").is_file());
    assert!(root.join("generated/types.ts").is_file());
    // Opt-in — unreachable from `all` before #335 §12. Looked for in the CACHE:
    // step 7 moved the ffi package out of the output directory, so the old
    // `generated/ffi` assertion would now fail no matter what the filter did.
    assert!(
        cache_ffi_package(home.path()).is_some(),
        "`all` did not reach the opt-in ffi target"
    );
}
