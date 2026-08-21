//! Guards for the derived-name scheme (#335 §2, epic #332).
//!
//! Scenario numbers refer to the BDD table in the accepted plan gate (#347).
//! Scenarios 1 and 2 live here; scenario 3 (two apps' FFI symbol sets are
//! disjoint) needs the ffi generator and lands with step 5b.
//!
//! **These vectors are load-bearing in the same way `cache_dir_test.rs`'s hash
//! vectors are.** `cargo install forgedb` builds the CLI with whatever toolchain
//! the user has, so a name that moves under a released cache re-keys every
//! package in it and the whole world silently recompiles. Do not update a vector
//! to match new output; work out why the output moved.

use std::path::Path;
use std::process::Command;

use forgedb::cache;
use forgedb::naming::{self, PackageKind};

// ---------------------------------------------------------------------------
// Scenario 2 — golden vectors over the composed names
// ---------------------------------------------------------------------------

/// The hashes here are the ones `cache_dir_test.rs::scenario_2_golden_hash_vectors`
/// pins. They are repeated verbatim rather than computed, so that a drift in
/// `member_hash` fails **both** files and neither can quietly adopt the other's
/// new answer.
const H_ROOT: &str = "60acb6cba9beb3cf"; // schema.forge
const H_API: &str = "4ec83b602ecd29f5"; // apps/api/schema.forge
const H_WEB: &str = "ad9a0dc7a10decf7"; // apps/web/schema.forge

#[test]
fn scenario_2_hashes_still_match_the_cache_vectors() {
    assert_eq!(cache::member_hash(Path::new("schema.forge")), H_ROOT);
    assert_eq!(cache::member_hash(Path::new("apps/api/schema.forge")), H_API);
    assert_eq!(cache::member_hash(Path::new("apps/web/schema.forge")), H_WEB);
}

#[test]
fn scenario_2_golden_package_names() {
    const VECTORS: &[(&str, &str, PackageKind, &str)] = &[
        ("schema.forge", H_ROOT, PackageKind::Core, "schema-60acb6cba9beb3cf-core"),
        ("schema.forge", H_ROOT, PackageKind::Server, "schema-60acb6cba9beb3cf-server"),
        ("apps/api/schema.forge", H_API, PackageKind::Ffi, "schema-4ec83b602ecd29f5-ffi"),
        ("apps/web/schema.forge", H_WEB, PackageKind::Wasm, "schema-ad9a0dc7a10decf7-wasm"),
    ];

    for (schema, hash, kind, expected) in VECTORS {
        let slug = naming::slug(Path::new(schema));
        assert_eq!(
            naming::package_name(&slug, hash, kind),
            *expected,
            "golden package-name vector moved for {schema} / {}",
            kind.dir()
        );
    }
}

/// The range is part of the name, not metadata beside it. One `transform/` per
/// app collides across ranges and `migrate run` gets whichever built last.
#[test]
fn scenario_2_golden_range_stamped_names() {
    let slug = naming::slug(Path::new("schema.forge"));

    assert_eq!(
        naming::package_name(&slug, H_ROOT, &PackageKind::Transform { from: 1, to: 2 }),
        "schema-60acb6cba9beb3cf-transform-1-2"
    );
    assert_eq!(
        naming::package_name(&slug, H_ROOT, &PackageKind::Engine { from: 1, to: 2 }),
        "schema-60acb6cba9beb3cf-engine-1-2"
    );
}

#[test]
fn scenario_2_golden_symbol_prefix() {
    let slug = naming::slug(Path::new("schema.forge"));
    assert_eq!(
        naming::symbol_prefix(&slug, H_ROOT),
        "schema_60acb6cba9beb3cf_"
    );
}

// ---------------------------------------------------------------------------
// Scenario 1 — a digit-leading hash still yields a cargo-legal package name
// ---------------------------------------------------------------------------

/// **This is the scenario the slug exists for**, and it is proven against real
/// cargo rather than against a rule written down here.
///
/// Cargo rejects a package name starting with a digit, and the rejection is
/// **project-wide**: `cargo metadata` exits 101, so the manifest is unreadable
/// rather than merely unbuildable. Six of sixteen hex digits are digits, so a
/// bare `<hash>-<kind>` scheme breaks for roughly three apps in eight while
/// passing whatever schema the implementer happened to test on.
///
/// `apps/api/schema.forge` hashes to `4ec8…`, which is exactly that case.
#[test]
fn scenario_1_digit_leading_hash_is_rescued_by_the_slug() {
    assert!(
        H_API.starts_with(|c: char| c.is_ascii_digit()),
        "this test is vacuous unless the fixture hash starts with a digit"
    );

    let slug = naming::slug(Path::new("apps/api/schema.forge"));
    let name = naming::package_name(&slug, H_API, &PackageKind::Core);

    assert!(
        name.starts_with(|c: char| c.is_ascii_alphabetic()),
        "{name} would be rejected by cargo"
    );

    // The bare scheme is rejected...
    assert_eq!(
        cargo_metadata_exit_code(&format!("{H_API}-core")),
        Some(101),
        "a bare <hash>-<kind> name should be rejected by cargo"
    );
    // ...and the slugged one is accepted.
    assert_eq!(
        cargo_metadata_exit_code(&name),
        Some(0),
        "{name} should be accepted by cargo"
    );
}

/// **The second, independent digit hazard: the schema's own file name.**
///
/// The slug rescues a digit-leading *hash* merely by existing — the hash is
/// never first in `<slug>-<hash>-<kind>`. That is the test above, and it passes
/// even if `slug` does no digit-forcing at all (verified by mutation). This one
/// is the other half: when the *schema file itself* is named `2024-orders.forge`
/// the slug would start with a digit and put a digit first in the package name.
///
/// Both were folded into one sentence in the design, and testing only the first
/// leaves the second live — a guard that is green for a reason unrelated to what
/// it claims.
#[test]
fn scenario_1_digit_leading_schema_name_is_cargo_legal() {
    let slug = naming::slug(Path::new("2024-orders.forge"));
    assert!(
        slug.starts_with(|c: char| c.is_ascii_alphabetic()),
        "slug {slug} starts with a digit"
    );

    // The unslugged stem is what cargo would have rejected.
    assert_eq!(
        cargo_metadata_exit_code("2024-orders-60acb6cba9beb3cf-core"),
        Some(101),
        "the un-rescued name should be rejected by cargo"
    );

    let name = naming::package_name(&slug, H_ROOT, &PackageKind::Core);
    assert_eq!(
        cargo_metadata_exit_code(&name),
        Some(0),
        "cargo rejected {name}"
    );
    // The rescue must not throw the information away — legibility is the slug's
    // only job, so `app` alone would be a regression rather than a fix.
    assert!(name.contains("2024-orders"), "{name} lost the schema's identity");
}

/// Every kind, for a digit-leading hash, is accepted by cargo. Checking only
/// `core` would leave the range-stamped names — the longest and least
/// conventional — unproven.
#[test]
fn scenario_1_every_kind_is_cargo_legal() {
    let slug = naming::slug(Path::new("apps/api/schema.forge"));
    let kinds = [
        PackageKind::Core,
        PackageKind::Server,
        PackageKind::Napi,
        PackageKind::Pyo3,
        PackageKind::Ffi,
        PackageKind::Wasm,
        PackageKind::Transform { from: 1, to: 2 },
        PackageKind::Engine { from: 12, to: 13 },
    ];

    for kind in kinds {
        let name = naming::package_name(&slug, H_API, &kind);
        assert_eq!(
            cargo_metadata_exit_code(&name),
            Some(0),
            "cargo rejected {name}"
        );
    }
}

/// Write a throwaway manifest declaring `name` and ask cargo to read it.
///
/// `cargo metadata` rather than `cargo build`: name legality is a manifest-parse
/// property, so this needs no compile, no network and no substrate — and it is
/// the same call the pre-build collision guard (§2) will make.
fn cargo_metadata_exit_code(name: &str) -> Option<i32> {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/lib.rs"), "").expect("lib.rs");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .expect("manifest");

    Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(dir.path())
        // An inherited CARGO_TARGET_DIR or a parent workspace must not reach
        // this — the same class of contamination #292 was.
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .ok()?
        .status
        .code()
}
