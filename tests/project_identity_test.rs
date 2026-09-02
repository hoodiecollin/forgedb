use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::project::{self, Chain, IdSource};

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");

const SCHEMA: &str = "Note {\n  id: +uuid\n  body: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn repo_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    root
}

fn run(cwd: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .output()
        .expect("forgedb binary runs")
}

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

fn project_line(out: &std::process::Output) -> String {
    combined(out)
        .lines()
        .find(|l| l.contains("Project:"))
        .map(|l| {
            l.split("Project:").nth(1).unwrap().trim().to_string()
        })
        .unwrap_or_else(|| panic!("no Project: line in output:\n{}", combined(out)))
}

#[test]
fn scenario_1_knobs_come_from_the_schemas_nearest_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"mono\"\n[storage]\nfsync = \"always\"\n",
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

#[test]
fn scenario_2_identity_comes_from_the_outermost_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");
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

    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    assert_eq!(chain.nearest().unwrap().dir, root.join("apps/api"));
    assert_eq!(chain.project_root().unwrap().dir, root);
}

#[test]
fn scenario_3_isolated_stops_the_walk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nid = \"api\"\nisolated = true\n[storage]\nfsync = \"never\"\n",
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

#[test]
fn scenario_4_omitted_isolated_means_isolated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");
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

#[test]
fn scenario_5_id_at_a_non_root_config_is_a_positioned_error() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nisolated = false\nversion = \"0.1.0\"\nid = \"api\"\n",
    );

    let err = project::identify(&Chain::walk(&root.join("apps/api")).unwrap())
        .expect_err("a nested config declaring an id is a contradiction");
    let msg = err.to_string();

    assert!(msg.contains("apps/api/forgedb.toml:4:1"), "positioned at the key: {msg}");
    assert!(msg.contains("[project].id"), "names the key: {msg}");
    assert!(msg.contains("isolated = true"), "offers the remedy: {msg}");
}

#[test]
fn scenario_6_a_declared_id_is_used_verbatim() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"chosen\"\n");
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"detected\"\nversion = \"0.1.0\"\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let id = project::identify(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(id.name, "chosen");
    assert_eq!(id.source, IdSource::Declared);
}

#[test]
fn scenario_7_manifest_names_are_never_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nisolated = true\n");
    write(
        &root.join("package.json"),
        "{ \"name\": \"storefront\", \"version\": \"1.0.0\" }",
    );
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let id = project::identify(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(id.source, IdSource::PathHash, "id was {:?}", id.name);
    assert!(
        !id.name.starts_with("storefront") && !id.name.starts_with("backend"),
        "a manifest name reached the id: {:?}",
        id.name
    );
}

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

#[test]
fn scenario_9_the_walk_stops_at_the_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().canonicalize().unwrap();
    write(&outside.join("forgedb.toml"), "[project]\nid = \"captured\"\n");

    let root = outside.join("repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nid = \"mine\"\n");
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    let chain = Chain::walk(&root.join("apps/api")).unwrap();
    assert_eq!(chain.links().len(), 1, "only the repo's own config");
    assert_eq!(project::identify(&chain).unwrap().name, "mine");
}

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

#[test]
fn scenario_11_an_unknown_table_errors() {
    let err = forgedb::config::parse_config(
        "[project]\nid = \"x\"\n\n[projekt]\nname = \"y\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("a misspelled table must not be ignored");
    let msg = err.to_string();
    assert!(msg.contains("line 4"), "positions the table: {msg}");
    assert!(msg.contains("projekt"), "names the table: {msg}");
}

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

#[test]
fn scenario_16_removed_generate_schema_has_its_own_diagnostic() {
    let err = forgedb::config::parse_config(
        "[project]\nid = \"x\"\n\n[generate]\noutput = \"./generated\"\nschema = \"schema.forge\"\n",
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

#[test]
fn scenario_13_a_taken_id_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("forgedb.toml"), "[project]\nid = \"clash\"\n");
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
    assert!(msg.contains("already held"), "{msg}");
    assert!(msg.contains("[project].id"), "names the remedy: {msg}");
}

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
    write(&base.join("one/forgedb.toml"), "[project]\nid = \"clash\"\n");
    write(&base.join("two/forgedb.toml"), "[project]\nid = \"resolved\"\n");

    let regenerate = |side: &str| {
        run(
            &base.join(side),
            home.path(),
            &["generate", "rust", "--force", "--output", base.join(side).join("generated").to_str().unwrap()],
        )
    };
    assert!(regenerate("one").status.success());
    assert!(regenerate("two").status.success());

    std::fs::remove_dir_all(home.path()).unwrap();
    std::fs::create_dir_all(home.path()).unwrap();

    let two = regenerate("two");
    assert!(two.status.success(), "the resolution survived the wipe:\n{}", combined(&two));
    let after = regenerate("one");
    assert!(after.status.success(), "…and did not merely swap who wins:\n{}", combined(&after));
}

#[test]
fn scenario_15_generate_and_build_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");
    write(
        &root.join("apps/api/forgedb.toml"),
        "[project]\nisolated = false\n[storage]\nfsync = \"never\"\n",
    );
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    let args = ["-v", "--schema", "apps/api/schema.forge"];
    let generate = run(&root, home.path(), &["-v", "generate", "rust", "--schema", "apps/api/schema.forge", "--output", root.join("out").to_str().unwrap()]);
    let build = run(&root, home.path(), &["build", "--plan", &args[0], "--schema", "apps/api/schema.forge"]);

    assert!(build.status.success(), "build --plan failed:\n{}", combined(&build));

    assert_eq!(project_line(&generate), "mono (from [project].id)");
    assert_eq!(
        project_line(&build),
        project_line(&generate),
        "build must resolve what generate resolved"
    );
}

#[test]
fn scenario_17_output_is_schema_relative() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"mono\"\n\n[generate]\noutput = \"generated\"\ntargets = [\"all\"]\n",
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

#[test]
fn init_inside_a_project_scaffolds_a_config_that_resolves() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");

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

    let id = project::identify(&Chain::walk(&root.join("api")).unwrap()).unwrap();
    assert_eq!(id.name, "mono");
}

#[test]
fn init_isolated_inside_a_project_names_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"mono\"\n");

    let out = run(&root, home.path(), &["init", "api", "--isolated"]);
    assert!(out.status.success(), "init failed:\n{}", combined(&out));

    let emitted = std::fs::read_to_string(root.join("api/forgedb.toml")).unwrap();
    assert!(emitted.contains("isolated = true"), "{emitted}");

    let id = project::identify(&Chain::walk(&root.join("api")).unwrap()).unwrap();
    assert!(
        id.name.starts_with("api-") && id.name.len() > "api-".len(),
        "minted id carries the slug and entropy: {:?}",
        id.name
    );
    assert!(emitted.contains(&format!("id = \"{}\"", id.name)), "{emitted}");
}

#[test]
fn init_into_a_directory_named_like_a_held_id_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    let first = base.join("first");
    std::fs::create_dir_all(first.join(".git")).unwrap();
    write(&first.join("forgedb.toml"), "[project]\nid = \"taken\"\n");
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
    assert!(
        out.status.success(),
        "a minted id cannot collide with a held one:\n{}",
        combined(&out)
    );

    let emitted = std::fs::read_to_string(elsewhere.join("taken/forgedb.toml")).unwrap();
    assert!(
        emitted.contains("id = \"taken-"),
        "the id carries the slug plus entropy, not the bare directory name: {emitted}"
    );
    assert!(
        !emitted.contains("id = \"taken\""),
        "the scaffold must not adopt the held id verbatim: {emitted}"
    );
}

#[test]
fn a_project_name_that_is_a_path_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);

    for hostile in ["../escape", "a/b", ""] {
        write(
            &root.join("forgedb.toml"),
            &format!("[project]\nid = \"{hostile}\"\n"),
        );
        let err = project::identify(&Chain::walk(&root).unwrap())
            .unwrap_err_or_else_name(hostile);
        assert!(err.contains("project id"), "{hostile}: {err}");
    }
}

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

#[test]
fn s335_16_absent_targets_is_a_positioned_error() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"needs-targets\"\n\n[generate]\noutput = \"generated\"\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "rust"]);
    assert!(!out.status.success(), "should have refused:\n{}", combined(&out));

    let msg = combined(&out);
    assert!(msg.contains("`[generate].targets` is required"), "{msg}");
    assert!(msg.contains("forgedb.toml:4:1"), "not positioned at the table: {msg}");
    assert!(msg.contains("targets = [\"all\"]"), "does not name the remedy: {msg}");
}

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

#[test]
fn s335_16_a_config_without_a_generate_table_still_generates() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"no-gen-table\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        cache_ffi_package(home.path()).is_some(),
        "an absent [generate] table did not take the built-in default:\n{}",
        combined(&out)
    );
}

#[test]
fn s335_17_an_unknown_target_value_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"bad-target\"\n\n[generate]\ntargets = [\"napi\"]\n",
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

#[test]
fn s335_19_a_deprecated_target_spelling_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"deprecated\"\n\n[generate]\ntargets = [\"typescript\", \"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    let msg = combined(&out);
    assert!(msg.contains("node-sdk"), "the warning does not name the replacement: {msg}");

    assert!(root.join("generated/types.ts").is_file(), "the TS output was not emitted");
}

#[test]
fn s335_18_all_reaches_the_opt_in_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"reach-all\"\n\n[generate]\ntargets = [\"all\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    assert!(root.join("generated/database.rs").is_file());
    assert!(root.join("generated/types.ts").is_file());
    assert!(
        cache_ffi_package(home.path()).is_some(),
        "`all` did not reach the opt-in ffi target"
    );
}

#[test]
fn s338_13a_a_misspelled_placement_key_errors() {
    let err = forgedb::config::parse_config(
        "[project]\nid = \"x\"\n\n[placement]\nrust_packge = \"generated/core\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("a misspelled key inside [placement] must not be ignored");
    let msg = err.to_string();
    assert!(msg.contains("line 5"), "positions the key: {msg}");
    assert!(msg.contains("rust_packge"), "names the key: {msg}");
}

#[test]
fn s338_13b_a_misspelled_placement_table_errors() {
    let err = forgedb::config::parse_config(
        "[project]\nid = \"x\"\n\n[placment]\nrust_package = \"generated/core\"\n",
        Path::new("forgedb.toml"),
    )
    .expect_err("a misspelled table must not be ignored");
    let msg = err.to_string();
    assert!(msg.contains("line 4"), "positions the table: {msg}");
    assert!(msg.contains("placment"), "names the table: {msg}");
}

#[test]
fn s338_13c_the_accepted_spelling_parses_and_absence_means_none() {
    let with = forgedb::config::parse_config(
        "[placement]\nrust_package = \"generated/core\"\n",
        Path::new("forgedb.toml"),
    )
    .expect("[placement] rust_package must parse");
    assert_eq!(with.placement.rust_package.as_deref(), Some("generated/core"));

    let without = forgedb::config::parse_config(
        "[project]\nid = \"x\"\n",
        Path::new("forgedb.toml"),
    )
    .expect("a config with no [placement] table must parse");
    assert_eq!(
        without.placement.rust_package, None,
        "absence of the table must not invent a placement"
    );
}
