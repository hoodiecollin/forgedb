//! The test harness's substrate list is anchored on the manifests ForgeDB
//! actually emits (#339).
//!
//! # What was stale
//!
//! `tests/common/mod.rs` hand-writes a `Cargo.toml` for the crate its driver
//! tests compile, and its dependency list used to be justified as "mirrors the
//! `forgedb init` server scaffold". Since #335 there is no scaffold manifest:
//! `init` writes no `Cargo.toml` at all, and the manifests that pin substrate are
//! the GENERATED `core/` and `server/`, rendered into ForgeDB's own build cache.
//!
//! A comment cannot fail. Replacing the claim with a check is the point of this
//! file — the relation it asserted was true when written, and nothing would have
//! reported it going false.
//!
//! # Superset, not equality
//!
//! A harness crate compiles one generated `database.rs` **and** one `api.rs` into
//! a single package, so it links what `core` and `server` link *together*, and
//! may legitimately carry a pin neither needs alone. The failure that matters is
//! the other direction: a NEW substrate dep appearing in an emitted manifest and
//! not in the harness, which surfaces as an unresolved-import compile error in
//! whichever driver test happens to exercise it — a failure that names the symbol
//! and not the cause.
//!
//! # Pure
//!
//! No fixture, no subprocess, no `generate`. The manifests are rendered in memory
//! by the same functions the emitters call, which is what makes this a statement
//! about the real renderer rather than about a copy of its output.

mod common;

use std::collections::BTreeSet;

use forgedb_codegen::{CorePackage, GenConfig, ServerPackage};

/// The `forgedb-*` dependency keys of a rendered manifest.
///
/// Reads the KEY at the start of a line, which is the form both renderers emit —
/// including inside `core`'s `[target.'cfg(not(target_arch = "wasm32"))']` table,
/// where `forgedb-coordinator` lives and where a scrape that only looked at
/// `[dependencies]` would miss it.
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

    // The scrape itself must not be vacuous: a renderer change that moved every
    // pin into a shape this parser does not recognise would otherwise make the
    // superset assertion trivially true.
    assert!(
        emitted.len() >= 8,
        "scraped only {} substrate pins from the emitted core+server manifests — \
         the parser has drifted from the renderers, and the superset check below \
         would now pass having compared against almost nothing. Got: {emitted:?}",
        emitted.len()
    );
    // `core` alone must contribute, and so must `server`: `server` is the only
    // carrier of forgedb-auth and forgedb-query-params anywhere.
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

/// Every name in the harness list resolves to a real crate in this workspace.
///
/// `common::dep` derives the directory by stripping the `forgedb-` prefix, so a
/// typo in the list produces a path dep pointing at nothing — which cargo reports
/// as a manifest error from a temp directory, at the point some unrelated driver
/// test runs. This says it here instead.
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
