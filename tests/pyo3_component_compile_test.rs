use std::path::{Path, PathBuf};
use std::process::Command;

const COMPONENT_SCHEMA: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
  card: tsx://components/user/card
  profile: jsx://components/profile @relations(*)
  verify: api://routes/user/verify
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

const CORE_PKG: &str = "s23-component-core";
const PYO3_PKG: &str = "s23-component-pyo3";

use forgedb::commands::generate::CORE_SUBSTRATE_REEXPORTS;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn patch_section() -> String {
    let root = repo_root();
    let mut out = String::from("\n[patch.crates-io]\n");
    for (krate, dir) in [
        ("forgedb-storage", "storage"),
        ("forgedb-types", "types"),
        ("forgedb-changefeed", "changefeed"),
        ("forgedb-wal", "wal"),
        ("forgedb-compaction", "compaction"),
        ("forgedb-txn", "txn"),
        ("forgedb-coordinator", "coordinator"),
    ] {
        let path = root.join("crates").join(dir);
        assert!(path.is_dir(), "no such substrate crate: {}", path.display());
        out.push_str(&format!(
            "{krate} = {{ path = {:?} }}\n",
            path.to_string_lossy()
        ));
    }
    out
}

#[test]
#[ignore = "compiles a generated cache workspace; run with --ignored"]
fn scenario_23_a_component_ref_schema_emits_pyo3_that_builds() {
    let mut parser = forgedb_parser::Parser::new(COMPONENT_SCHEMA).unwrap();
    let schema = parser.parse().unwrap();

    let user = &schema.models[0];
    assert!(
        user.fields.iter().any(|f| matches!(
            f.field_type,
            forgedb_parser::FieldType::Component(_)
        )),
        "the fixture schema has no component ref"
    );

    let config = forgedb_codegen::GenConfig::DEFAULT;
    let database = forgedb_codegen::RustGenerator::generate_with_config(&schema, 1, config)
        .unwrap()
        .code;
    let wrapper = forgedb_codegen::PyO3Generator::generate(&schema).unwrap().code;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    write(
        &root.join("Cargo.toml"),
        &format!(
            "[workspace]\nresolver = \"3\"\nmembers = [\"core\", \"pyo3\"]\n{}",
            patch_section()
        ),
    );

    write(
        &root.join("core/Cargo.toml"),
        &forgedb_codegen::CorePackage::cargo_toml(CORE_PKG, &config),
    );
    write(
        &root.join("core/src/lib.rs"),
        &format!("{database}{CORE_SUBSTRATE_REEXPORTS}"),
    );

    write(
        &root.join("pyo3/Cargo.toml"),
        &forgedb_codegen::PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG),
    );
    write(&root.join("pyo3/src/lib.rs"), &wrapper);
    write(
        &root.join("pyo3/build.rs"),
        forgedb_codegen::PyO3Generator::build_rs_scaffold(),
    );

    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "-p", PYO3_PKG, "--target-dir"])
        .arg(root.join("target"))
        .current_dir(root)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo runs");

    assert!(
        out.status.success(),
        "the generated pyo3 wrapper does not build:\n--- stderr ---\n{}\n--- lib.rs ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        wrapper,
    );

    let libdir = root.join("target/debug");
    let produced: Vec<String> = std::fs::read_dir(&libdir)
        .expect("target/debug exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let stem = PYO3_PKG.replace('-', "_");
    assert!(
        produced.iter().any(|n| {
            n.contains(&stem)
                && (n.ends_with(".dylib") || n.ends_with(".so") || n.ends_with(".dll"))
        }),
        "no cdylib was linked for {PYO3_PKG}; the build did not reach the linker: {produced:?}"
    );
}
