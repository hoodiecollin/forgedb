//! The migration lineage is read from the SCHEMA's directory, never the CWD (#437).
//!
//! `generate` bakes the lineage's serial into the generated app's
//! `EXPECTED_SCHEMA_VERSION`, and the open guard refuses a data dir whose stamp disagrees.
//! `migrate` writes that lineage to `<schema_dir>/migrations`. If the two disagree about
//! where it lives, the interlock guards nothing — and it fails *silently*, because both
//! halves still compile and both still produce a number.
//!
//! ## Why the fixture has a decoy, and why it is two levels deep
//!
//! The obvious test — generate from the parent, assert the version is not the baseline —
//! passes for the wrong reason: with no `migrations/` in the CWD the old code fell back to
//! baseline 1, so "not baseline" and "read the right directory" look identical.
//!
//! They are not. The old call read whatever `migrations/` the current directory *had*. So
//! the sharp case is a **decoy**: a second, different lineage sitting in the directory you
//! happen to be standing in. Generating app B from app A's directory baked A's serial into
//! B. A fixture without a decoy cannot see that, and a decoy is only meaningful if the two
//! directories are genuinely distinct — hence two levels.
//!
//! Tier 1: these invoke the built binary as a subprocess with an explicit `current_dir` and
//! compile nothing, so they cost a process, not a crate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb_migrations::{Migration, SchemaChange};

const SCHEMA: &str = r#"Widget {
  id: +uuid
  name: string
}
"#;

fn forgedb_bin() -> PathBuf {
    // The integration-test binary sits beside the CLI it is testing.
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("forgedb")
}

/// Write a lineage under `dir` whose highest `to_version` is `version`.
///
/// Built through `Migration::new_versioned` and serialized the way `MigrationGenerator`
/// serializes, rather than hand-rolled JSON, so the record carries a real checksum and this
/// fixture cannot drift from the format it is standing in for.
fn write_lineage(dir: &Path, version: u32) {
    fs::create_dir_all(dir).expect("create lineage dir");
    let m = Migration::new_versioned(
        format!("fixture to v{version}"),
        vec![SchemaChange::AddModel {
            model_name: "Widget".to_string(),
        }],
        version - 1,
        version,
    );
    let json = serde_json::to_string_pretty(&m).expect("serialize migration");
    fs::write(dir.join(m.filename()), json).expect("write migration");
}

/// A project whose schema is one level down from the config root, with a real lineage
/// beside the schema and — optionally — a *different* lineage at the root to stand in for
/// "some other app's migrations, in the directory you happen to be standing in".
struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    app: PathBuf,
}

fn fixture(app_version: u32, decoy_version: Option<u32>) -> Fixture {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let app = root.join("app");
    fs::create_dir_all(&app).expect("create app dir");

    fs::write(root.join("forgedb.toml"), "[project]\nid = \"lineage-cwd-fixture\"\n")
        .expect("write config");
    fs::write(app.join("schema.forge"), SCHEMA).expect("write schema");

    write_lineage(&app.join("migrations"), app_version);
    if let Some(v) = decoy_version {
        write_lineage(&root.join("migrations"), v);
    }

    Fixture { _tmp: tmp, root, app }
}

/// Run `generate rust` from `cwd` and return the `EXPECTED_SCHEMA_VERSION` it baked.
fn baked_version(fx: &Fixture, cwd: &Path, schema_arg: &str) -> u32 {
    let out = fx.root.join("out");
    let result = Command::new(forgedb_bin())
        .current_dir(cwd)
        .args(["generate", "rust", "--schema", schema_arg, "--output"])
        .arg(&out)
        .arg("--force")
        // Keep the build cache out of the developer's real home.
        .env("FORGEDB_HOME", fx.root.join(".forgedb-home"))
        .output()
        .expect("run forgedb generate");

    assert!(
        result.status.success(),
        "generate failed from {}:\n{}\n{}",
        cwd.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );

    let src = fs::read_to_string(out.join("database.rs")).expect("read database.rs");
    let needle = "const EXPECTED_SCHEMA_VERSION: u32 = ";
    let at = src
        .find(needle)
        .unwrap_or_else(|| panic!("no EXPECTED_SCHEMA_VERSION in generated database.rs"));
    let rest = &src[at + needle.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().expect("parse baked version")
}

/// The in-run control: standing in the schema's own directory, the serial is correct.
///
/// This is what made the bug invisible — the one CWD everybody tests from is the one CWD
/// where the old code was right.
#[test]
fn from_the_schemas_own_directory_the_lineage_is_read() {
    let fx = fixture(7, None);
    assert_eq!(baked_version(&fx, &fx.app, "schema.forge"), 7);
}

/// The bug: generating from anywhere else.
#[test]
fn from_a_parent_directory_the_lineage_is_still_the_schemas() {
    let fx = fixture(7, None);
    assert_eq!(
        baked_version(&fx, &fx.root, "app/schema.forge"),
        7,
        "the lineage must come from the schema's directory. Reading it relative to the CWD \
         finds no `migrations/` here and silently falls back to baseline 1 — the generated \
         app then believes it is at v1 and the open guard stops guarding."
    );
}

/// The sharp case, and the one a no-decoy fixture cannot see.
///
/// A different lineage sits in the CWD. The old code read *that* one, so generating app B
/// from app A's directory baked A's serial into B. Both numbers are non-baseline, so
/// "didn't fall back to 1" would have passed here while the value was still wrong.
#[test]
fn a_lineage_in_the_cwd_does_not_shadow_the_schemas() {
    let fx = fixture(7, Some(3));
    assert_eq!(
        baked_version(&fx, &fx.root, "app/schema.forge"),
        7,
        "a `migrations/` directory in the CWD must not be read: it belongs to whatever \
         project is rooted there, not to the schema being generated"
    );
}
