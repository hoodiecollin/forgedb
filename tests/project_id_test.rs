//! **#479 — the id is minted, not derived.**
//!
//! The three properties nothing else covers: the path-hash fallback names its
//! remedy, `forgedb project show` reports the facts a user actually needs, and
//! the four surfaces this issue removed are gone.
//!
//! The rest of the change is guarded where it already lived —
//! `project_identity_test` (resolution order, the nested-`id` contradiction, the
//! manifest names that are no longer adopted), `init_scaffold_test` (the minted
//! value), `prompt_boundary_test` (the collision refusal), `lineage_cwd_test`
//! (the walk still starting at the schema).

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");

const SCHEMA: &str = "Post {\n  id: +uuid\n  title: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A directory that stops the walk, so a test never climbs into the real repo.
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

/// A project with no `[project].id` still resolves — and says what it cost.
///
/// The fallback is not a failure: a path hash cannot collide with itself, so it
/// is a perfectly good key. What it is not is *stable*, and the warning has to
/// say which — "it changes if the directory moves" is the consequence, and the
/// remedy names the key **and a usable value**, because a user told to add an id
/// with no idea what shape one takes will invent one.
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

/// `show` reports the two facts that used to require knowing the layout.
///
/// **`Cache:`** is the directory the id keys — the whole reason identity exists,
/// and previously findable only by knowing `~/.forgedb/projects/<id>/`.
/// **`Schema:`** is which app answered: `-s` selects the chain, so in a monorepo
/// two correct-looking reports differ by that line alone.
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

/// **The removed surface.** Four spellings #479 deleted, each refused.
///
/// These are *clap* refusals, deliberately unlike the tombstones in
/// `removed_surface_test` — that file's doctrine is that a removed surface must
/// error naming its replacement rather than being answered by clap, and it is
/// right, because those removals had shipped and user scripts pass them. None of
/// these four ever shipped: `src/project.rs` does not exist in v0.4.1 and
/// released ForgeDB ignored the `[project]` table entirely. Nothing outside this
/// repository has ever typed them, so there is nobody to name a replacement to.
///
/// What this asserts is that they are *gone* — non-zero exit, and no silent
/// acceptance. A removed flag that parses and does nothing is the failure both
/// files exist to prevent.
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

    // `init --project-name` separately, because it must ALSO not scaffold.
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
