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
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("forgedb")
}

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

fn baked_version(fx: &Fixture, cwd: &Path, schema_arg: &str) -> u32 {
    let out = fx.root.join("out");
    let result = Command::new(forgedb_bin())
        .current_dir(cwd)
        .args(["generate", "rust", "--schema", schema_arg, "--output"])
        .arg(&out)
        .arg("--force")
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

#[test]
fn from_the_schemas_own_directory_the_lineage_is_read() {
    let fx = fixture(7, None);
    assert_eq!(baked_version(&fx, &fx.app, "schema.forge"), 7);
}

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
