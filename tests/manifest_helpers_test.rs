mod common;

const MANIFEST: &str = r#"
[package]
name = "app-core"
version = "0.1.0"

[dependencies]
forgedb-types = "0.3"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
insta = "1"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
forgedb-coordinator = "0.2"

[workspace]
members = ["apps/a/core", "apps/a/wasm"]
default-members = []
"#;

#[test]
fn dependency_keys_come_from_every_dependencies_table_including_target_scoped_ones() {
    let keys = common::manifest_dep_keys(MANIFEST);
    for want in ["forgedb-types", "serde", "insta", "forgedb-coordinator"] {
        assert!(keys.contains(want), "missing {want} in {keys:?}");
    }
    assert!(!keys.contains("name"), "a [package] key is not a dependency: {keys:?}");
    assert!(!keys.contains("members"), "a [workspace] key is not a dependency: {keys:?}");
}

#[test]
fn string_and_array_lookups_distinguish_absent_from_empty() {
    assert_eq!(
        common::manifest_str(MANIFEST, &["package", "name"]).as_deref(),
        Some("app-core")
    );
    assert_eq!(common::manifest_str(MANIFEST, &["package", "nope"]), None);
    assert_eq!(
        common::manifest_str_array(MANIFEST, &["workspace", "members"]),
        vec!["apps/a/core", "apps/a/wasm"]
    );
    assert!(common::manifest_str_array(MANIFEST, &["workspace", "default-members"]).is_empty());
    assert!(common::manifest_str_array(MANIFEST, &["workspace", "exclude"]).is_empty());
}

#[test]
#[should_panic(expected = "not valid TOML")]
fn a_manifest_that_does_not_parse_is_fatal_not_empty() {
    common::manifest_dep_keys("[dependencies\nbroken = ");
}
