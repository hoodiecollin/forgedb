//! The persisting act — the #367 scenarios (gate #371, S6–S12).
//!
//! **Every scenario here runs with piped stdio**, which is to say with no
//! terminal anywhere. That is not a limitation being worked around: the whole
//! design exists so the *act* is separable from the *widget*, and these are what
//! prove the separation. `forgedb project name` is the answer to the question,
//! not a side effect of asking it.

use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::ask::{CommandConsent, NeverAsk};
use forgedb::project::{self, Chain};

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

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A root two ecosystem manifests name, with a schema and no `forgedb.toml`.
fn ambiguous_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = repo_root(dir);
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("package.json"),
        "{ \"name\": \"storefront\", \"version\": \"1.0.0\" }",
    );
    write(&root.join("schema.forge"), SCHEMA);
    root
}

/// Every file under `dir`, as `(project-relative path, bytes)`.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    collect(dir, dir, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push((rel.to_path_buf(), std::fs::read(&path).unwrap_or_default()));
        }
    }
}

// ---------------------------------------------------------------------------
// S6 — a config is created where none exists
// ---------------------------------------------------------------------------

/// The common instance of decision 1, and the plain one: there is no file, so
/// persisting is a *create*. Nothing to preserve, nothing to clobber, no
/// format-preserving editor needed, and no ownership question — which is why
/// gate 1 split the "may ForgeDB write a config it did not author?" decision
/// rather than answering it once.
#[test]
fn s6_a_config_is_created_where_none_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = ambiguous_root(&tmp);

    let out = run(&root, home.path(), &["project", "name", "storefront"]);
    assert!(out.status.success(), "{}", combined(&out));

    let config = root.join("forgedb.toml");
    let text = std::fs::read_to_string(&config).expect("forgedb.toml was created");
    assert!(text.contains("name = \"storefront\""), "{text}");
    assert!(
        text.contains("[project]"),
        "the name is under [project]: {text}"
    );
    // The provenance line — the difference between a mystery file appearing in
    // `git status` and a legible one.
    assert!(
        text.contains("forgedb project name"),
        "a created config says what created it: {text}"
    );
    // Deliberately NOT written: `parse_config` refuses `generate.targets = None`,
    // which reads like a `[project]`-only file must hard-error. It does not —
    // `GenerateConfig::default()` states `["all"]` for a file with no
    // `[generate]` table at all — so writing targets "to be safe" would be
    // ForgeDB putting a knob it does not need into someone else's repository.
    assert!(
        !text.contains("targets"),
        "a created config states only the identity it was asked for: {text}"
    );

    // …and the ambiguity is gone, for a *later, independent* invocation with no
    // flags at all. That is the whole point: an answer in one `argv` would not
    // survive this.
    let after = run(
        &root,
        home.path(),
        &[
            "-v",
            "generate",
            "rust",
            "--output",
            root.join("generated").to_str().unwrap(),
        ],
    );
    assert!(after.status.success(), "{}", combined(&after));
    assert!(
        combined(&after).contains("Project: storefront (from [project].name)"),
        "{}",
        combined(&after)
    );
}

// ---------------------------------------------------------------------------
// S7 — creating a config changes nothing else
// ---------------------------------------------------------------------------

/// Creating a `forgedb.toml` flips `Chain::nearest()` from `None` to `Some`, so
/// **every knob then comes from a parsed file instead of from
/// `ForgeConfig::default()`**.
///
/// They agree today, field by field. Reading the struct cannot prove that and
/// would not survive a future field being added on one side only — the byte
/// comparison can and does.
#[test]
fn s7_creating_a_config_changes_no_generated_byte() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    // Exactly ONE naming manifest, so this generates before as well as after.
    write(
        &root.join("package.json"),
        "{ \"name\": \"solo\", \"version\": \"1.0.0\" }",
    );
    write(&root.join("schema.forge"), SCHEMA);
    let out_dir = root.join("generated");

    let first = run(
        &root,
        home.path(),
        &["generate", "all", "--output", out_dir.to_str().unwrap()],
    );
    assert!(first.status.success(), "{}", combined(&first));
    let before = snapshot(&out_dir);
    assert!(!before.is_empty(), "something was generated");

    // Record the SAME name the manifest already supplied — so the only change
    // is the existence of the file.
    let record = run(&root, home.path(), &["project", "name", "solo"]);
    assert!(record.status.success(), "{}", combined(&record));

    let second = run(
        &root,
        home.path(),
        &[
            "generate",
            "all",
            "--force",
            "--output",
            out_dir.to_str().unwrap(),
        ],
    );
    assert!(second.status.success(), "{}", combined(&second));
    assert_eq!(
        before,
        snapshot(&out_dir),
        "a created config must change no generated byte"
    );
}

// ---------------------------------------------------------------------------
// S8 — an existing config is edited format-preservingly, and only with consent
// ---------------------------------------------------------------------------

/// The narrower case: a config exists and lacks a name. Editing it is a
/// different act from creating one, and takes explicit in-session consent.
///
/// Consent lives on the *asker* rather than on a flag because the same
/// `record_name` call has to be a refusal when a `generate` reaches it and an
/// authorised edit when the user asked for it by name.
#[test]
fn s8_an_existing_config_is_edited_only_with_consent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    let config = root.join("forgedb.toml");
    let original = "# hand-written, and the user owns every byte of it\n\
                    [project]\n\
                    isolated = true\n\
                    \n\
                    # which targets this app declares\n\
                    [generate]\n\
                    targets = [\"api\"]\n";
    write(&config, original);
    write(&root.join("schema.forge"), SCHEMA);

    let chain = Chain::walk_from_schema(&root.join("schema.forge")).unwrap();

    // No consent → an error naming the file and the exact key, and NOT A BYTE
    // written. "Cannot ask" and "declined" are the same path by construction.
    let err = project::record_name(&chain, "picked", false, &NeverAsk)
        .expect_err("an unconsented edit is refused");
    let msg = err.to_string();
    assert!(msg.contains("forgedb.toml"), "{msg}");
    assert!(msg.contains("name = \"picked\""), "names the key: {msg}");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        original,
        "a refusal writes nothing"
    );

    // With consent — which is what typing `forgedb project name` means.
    let recorded = project::record_name(&chain, "picked", false, &CommandConsent).unwrap();
    assert!(!recorded.created, "this was an edit, not a create");
    let after = std::fs::read_to_string(&config).unwrap();
    assert!(after.contains("name = \"picked\""), "{after}");
    assert!(
        after.contains("# hand-written, and the user owns every byte of it"),
        "comments survive a format-preserving edit: {after}"
    );
    assert!(
        after.contains("# which targets this app declares"),
        "…all of them: {after}"
    );
    assert!(
        after.contains("targets = [\"api\"]"),
        "and every other table: {after}"
    );

    // The edited file still parses — the write is applied only after a re-parse
    // through the same `parse_config` every read uses.
    let re = Chain::walk_from_schema(&root.join("schema.forge")).unwrap();
    assert_eq!(project::identify(&re).unwrap().name, "picked");
}

/// The same refusal from the CLI's non-interactive side: a piped `generate`
/// over a config that declares no name errors and touches nothing.
#[test]
fn s8b_a_piped_generate_never_edits_a_config() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    let config = root.join("forgedb.toml");
    let original = "[project]\nisolated = true\n";
    write(&config, original);
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("package.json"),
        "{ \"name\": \"storefront\", \"version\": \"1.0.0\" }",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = run(
        &root,
        home.path(),
        &[
            "generate",
            "rust",
            "--output",
            root.join("generated").to_str().unwrap(),
        ],
    );
    assert!(!out.status.success(), "{}", combined(&out));
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        original,
        "a failing generate must not have written to the user's config"
    );
}

// ---------------------------------------------------------------------------
// S9 — an existing name is never overwritten silently
// ---------------------------------------------------------------------------

/// A rename is a legitimate request and a silent clobber is not. The difference
/// is `--force`, and the cost — a build cache keyed on the old id — is reported
/// rather than left to be discovered as a directory nobody can account for.
#[test]
fn s9_an_existing_name_is_never_overwritten_silently() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    let config = root.join("forgedb.toml");
    write(&config, "[project]\nname = \"already\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    // Make the old id's cache directory real, so the orphan warning has
    // something to point at.
    let first = run(
        &root,
        home.path(),
        &[
            "generate",
            "rust",
            "--output",
            root.join("generated").to_str().unwrap(),
        ],
    );
    assert!(first.status.success(), "{}", combined(&first));
    assert!(home.path().join("projects/already").exists());

    let refused = run(&root, home.path(), &["project", "name", "other"]);
    assert!(!refused.status.success(), "a silent clobber is refused");
    let msg = combined(&refused);
    assert!(msg.contains("--force"), "names the remedy: {msg}");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "[project]\nname = \"already\"\n",
        "a refusal writes nothing"
    );

    let forced = run(&root, home.path(), &["project", "name", "other", "--force"]);
    assert!(forced.status.success(), "{}", combined(&forced));
    let after = std::fs::read_to_string(&config).unwrap();
    assert!(after.contains("name = \"other\""), "{after}");
    let report = combined(&forced);
    assert!(
        report.contains("already") && report.contains("orphaned"),
        "a rename reports the cache directory it orphaned: {report}"
    );
}
