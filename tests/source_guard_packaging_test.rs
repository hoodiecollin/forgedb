use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn manifest(rel: &str) -> toml::Value {
    let path = repo_root().join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.parse::<toml::Value>()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

const CONSUMERS: [&str; 2] = ["Cargo.toml", "crates/codegen/Cargo.toml"];

#[test]
fn the_testkit_is_never_published() {
    let m = manifest("crates/source-guard/Cargo.toml");
    let publish = m
        .get("package")
        .and_then(|p| p.get("publish"))
        .unwrap_or_else(|| panic!("crates/source-guard must set `publish` explicitly"));

    assert_eq!(
        publish.as_bool(),
        Some(false),
        "forgedb-source-guard is a dev-only testkit and must never reach crates.io; \
         if this ever becomes publishable, the version-key rule below inverts"
    );
}

#[test]
fn consumers_declare_the_testkit_without_a_version() {
    let mut checked = 0;

    for rel in CONSUMERS {
        let m = manifest(rel);
        let dep = m
            .get("dev-dependencies")
            .and_then(|d| d.get("forgedb-source-guard"))
            .unwrap_or_else(|| {
                panic!("{rel} is listed as a testkit consumer but does not declare it")
            });

        assert!(
            dep.get("path").is_some(),
            "{rel}: the testkit must be a path dependency — it is not published"
        );
        assert!(
            dep.get("version").is_none(),
            "{rel}: the `forgedb-source-guard` dev-dependency MUST NOT carry a `version` key. \
             Cargo only strips a versionless dev-dependency from the published manifest, so a \
             version here breaks `cargo publish` for this crate with \"no matching package \
             found\". Every other path dep here has a version because every other one is \
             published; this one is the deliberate exception."
        );
        checked += 1;
    }

    assert_eq!(
        checked,
        CONSUMERS.len(),
        "every listed consumer must actually have been checked"
    );
    assert!(checked > 0, "the consumer list must not be empty");
}

#[test]
fn the_testkit_is_a_workspace_member() {
    let m = manifest("Cargo.toml");
    let members = m["workspace"]["members"]
        .as_array()
        .expect("workspace.members is an array");

    assert!(
        members
            .iter()
            .any(|v| v.as_str() == Some("crates/source-guard")),
        "crates/source-guard must be in workspace.members, or `cargo test --workspace` \
         never runs its tests"
    );
}
