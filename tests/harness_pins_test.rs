mod common;

use std::collections::BTreeSet;

use forgedb_codegen::{CorePackage, GenConfig, ServerPackage};

fn substrate_keys(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
        .filter(|k| k.starts_with("forgedb-"))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_harness_pins_a_superset_of_what_forgedb_emits() {
    let config = GenConfig::default();
    let core = CorePackage::cargo_toml("app-core", &config);
    let server = ServerPackage::cargo_toml("app-server", "app-core");

    let mut emitted = substrate_keys(&core);
    emitted.extend(substrate_keys(&server));

    assert!(
        emitted.len() >= 8,
        "scraped only {} substrate pins from the emitted core+server manifests — \
         the parser has drifted from the renderers, and the superset check below \
         would now pass having compared against almost nothing. Got: {emitted:?}",
        emitted.len()
    );
    assert!(
        substrate_keys(&server).contains("forgedb-auth"),
        "the emitted `server` manifest no longer pins forgedb-auth; it is the only \
         carrier of that crate, here and in the reclose"
    );

    let harness: BTreeSet<String> = common::SUBSTRATE_PINS.iter().map(|s| s.to_string()).collect();
    let missing: Vec<_> = emitted.difference(&harness).collect();

    assert!(
        missing.is_empty(),
        "the emitted manifests pin substrate the test harness does not: {missing:?}.\n\
         `tests/common/mod.rs` compiles a generated `database.rs` + `api.rs` against \
         the working tree, so a crate ForgeDB now requires and the harness does not \
         supply surfaces as an unresolved-import error in whichever driver test \
         happens to touch it — naming the symbol, never the cause.\n\
         Add it to SUBSTRATE_PINS (the crate directory is derived from the name)."
    );
}

#[test]
fn every_harness_pin_names_a_crate_in_this_workspace() {
    for name in common::SUBSTRATE_PINS {
        let dir = name
            .strip_prefix("forgedb-")
            .unwrap_or_else(|| panic!("SUBSTRATE_PINS entry {name:?} is not a `forgedb-` crate"));
        let manifest = common::repo_root().join("crates").join(dir).join("Cargo.toml");
        assert!(
            manifest.is_file(),
            "SUBSTRATE_PINS names {name}, but {} does not exist. The directory is \
             DERIVED from the name — every substrate crate lives at \
             `crates/<name without the forgedb- prefix>`.",
            manifest.display()
        );
    }
}
