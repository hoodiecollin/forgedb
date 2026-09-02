use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");

const SCHEMA: &str = "Post {\n  id: +uuid\n  title: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn repo_root(dir: &tempfile::TempDir) -> std::path::PathBuf {
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

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_project_without_an_id_falls_back_and_names_the_remedy() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(&root, home.path(), &["project", "show"]);
    let msg = combined(&out);

    assert!(out.status.success(), "the fallback resolves rather than errors:\n{msg}");
    assert!(msg.contains("[project].id"), "names the key: {msg}");
    assert!(
        msg.contains("changes if the directory moves"),
        "names the consequence rather than merely the key: {msg}"
    );
    assert!(
        !msg.contains("[project].name"),
        "the deleted key must not survive in a diagnostic: {msg}"
    );
}

#[test]
fn show_reports_the_cache_directory_and_the_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"reported\"\n");
    write(&root.join("apps/api/schema.forge"), SCHEMA);

    let out = run(
        &root,
        home.path(),
        &["project", "show", "--schema", "apps/api/schema.forge"],
    );
    let msg = combined(&out);
    assert!(out.status.success(), "{msg}");

    assert!(msg.contains("Id:           reported"), "{msg}");
    assert!(
        msg.contains(&home.path().join("projects/reported").display().to_string()),
        "`Cache:` must be the directory the id keys, not merely the id: {msg}"
    );
    assert!(
        msg.contains("apps/api/schema.forge"),
        "`Schema:` must name the app that answered: {msg}"
    );
}

#[test]
fn the_removed_identity_surface_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"removed\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    for args in [
        vec!["project", "name", "something"],
        vec!["project", "claim", "--take-over"],
        vec!["project", "release"],
    ] {
        let out = run(&root, home.path(), &args);
        assert!(
            !out.status.success(),
            "`forgedb {}` must be refused:\n{}",
            args.join(" "),
            combined(&out)
        );
    }

    let elsewhere = tmp.path().join("scaffold-target");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let out = run(
        &elsewhere,
        home.path(),
        &["init", "app", "--project-name", "chosen"],
    );
    assert!(
        !out.status.success(),
        "`init --project-name` must be refused:\n{}",
        combined(&out)
    );
    assert!(
        !elsewhere.join("app").exists(),
        "a refused init must leave nothing behind"
    );
}
