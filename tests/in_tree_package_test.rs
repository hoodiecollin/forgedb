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

fn package_name(manifest: &Path) -> String {
    let body = read(manifest);
    let value: toml::Value = toml::from_str(&body)
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}\n{body}", manifest.display()));
    value["package"]["name"]
        .as_str()
        .expect("[package] name")
        .to_string()
}

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

    let cache = container(root, "s1");
    assert!(cache.join("core/Cargo.toml").is_file());
    assert!(cache.join("core/src/lib.rs").is_file());
    assert!(root.join("generated/database.rs").is_file());
}

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

    assert!(
        value.get("workspace").is_none(),
        "the emitted package must carry no [workspace] table:\n{manifest}"
    );
}

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

#[test]
fn scenario_7_a_hand_edit_is_rewritten_in_full() {
    let tmp = project("s7", "\"rust\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "first generate");

    let manifest = root.join("generated/core/Cargo.toml");
    let lib = root.join("generated/core/src/lib.rs");
    let before = read(&manifest);

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

#[test]
fn scenarios_9_and_17_build_regenerates_the_package_but_never_plans_it() {
    let tmp = project("s9", "\"rust\", \"api\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let lib = root.join("generated/core/src/lib.rs");
    write(&lib, "// stale\n");

    let out = ok(&forgedb(root, &["build", "--plan"]), "build --plan");

    assert_eq!(
        read(&lib),
        read(&root.join("generated/database.rs")),
        "`build` left the in-tree package stale"
    );

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

#[test]
fn scenario_8_check_compares_and_writes_nothing() {
    let tmp = project("s8", "\"rust\"", PLACEMENT);
    let root = tmp.path();
    ok(&forgedb(root, &["generate", "all", "--force"]), "generate all");

    let lib = root.join("generated/core/src/lib.rs");
    let manifest = root.join("generated/core/Cargo.toml");

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

#[test]
fn scenario_12_a_placement_inside_the_cache_is_refused() {
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

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

    let members = metadata_members(ws, &target_dir);
    let core = package_name(&ws.join("generated/core/Cargo.toml"));
    assert!(
        members.contains(&core),
        "the generated package did not join the workspace: {members:?} (wanted {core})"
    );
    assert!(members.contains(&"consumer".to_string()));
    assert!(members.contains(&"helper".to_string()));
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
