use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::cache;
use forgedb::naming::{self, PackageKind, SymbolNaming};

fn paths(raw: &[&str]) -> Vec<PathBuf> {
    raw.iter().map(PathBuf::from).collect()
}

const H_ROOT: &str = "60acb6cba9beb3cf";
const H_API: &str = "4ec83b602ecd29f5";
const H_WEB: &str = "ad9a0dc7a10decf7";

#[test]
fn scenario_2_hashes_still_key_the_member_directory() {
    assert_eq!(cache::member_hash(Path::new("schema.forge")), H_ROOT);
    assert_eq!(cache::member_hash(Path::new("apps/api/schema.forge")), H_API);
    assert_eq!(cache::member_hash(Path::new("apps/web/schema.forge")), H_WEB);
}

#[test]
fn scenario_2_golden_app_names_minimal() {
    let project = paths(&[
        "services/blog/schema.forge",
        "services/cart/schema.forge",
        "app/blog/schema.forge",
        "app/blog/api.forge",
    ]);

    const VECTORS: &[(&str, &str)] = &[
        ("services/blog/schema.forge", "foo_services_blog"),
        ("services/cart/schema.forge", "foo_cart"),
        ("app/blog/schema.forge", "foo_app_blog"),
        ("app/blog/api.forge", "foo_api"),
    ];

    for (rel, want) in VECTORS {
        assert_eq!(
            naming::app_name("foo", Path::new(rel), &project, SymbolNaming::Minimal),
            *want,
            "minimal name for {rel}"
        );
    }
}

#[test]
fn scenario_2_golden_app_names_uniform() {
    let project = paths(&[
        "services/blog/schema.forge",
        "services/cart/schema.forge",
        "app/blog/schema.forge",
        "app/blog/api.forge",
    ]);

    const VECTORS: &[(&str, &str)] = &[
        ("services/blog/schema.forge", "foo_services_blog"),
        ("services/cart/schema.forge", "foo_services_cart"),
        ("app/blog/schema.forge", "foo_app_blog"),
        ("app/blog/api.forge", "foo_app_blog_api"),
    ];

    for (rel, want) in VECTORS {
        assert_eq!(
            naming::app_name("foo", Path::new(rel), &project, SymbolNaming::Uniform),
            *want,
            "uniform name for {rel}"
        );
    }
}

#[test]
fn scenario_2_golden_app_names_shallow_project() {
    let project = paths(&[
        "services/blog/schema.forge",
        "services/cart/schema.forge",
        "api/schema.forge",
    ]);

    const VECTORS: &[(&str, &str)] = &[
        ("services/blog/schema.forge", "bar_blog"),
        ("services/cart/schema.forge", "bar_cart"),
        ("api/schema.forge", "bar_api"),
    ];

    for (rel, want) in VECTORS {
        assert_eq!(
            naming::app_name("bar", Path::new(rel), &project, SymbolNaming::Minimal),
            *want,
            "minimal name for {rel}"
        );
    }
}

#[test]
fn scenario_2_golden_package_names() {
    const VECTORS: &[(&str, PackageKind, &str)] = &[
        ("foo_services_blog", PackageKind::Core, "foo_services_blog-core"),
        ("foo_services_blog", PackageKind::Server, "foo_services_blog-server"),
        ("bar_api", PackageKind::Ffi, "bar_api-ffi"),
        ("bar_api", PackageKind::Wasm, "bar_api-wasm"),
    ];

    for (app, kind, want) in VECTORS {
        assert_eq!(naming::package_name(app, kind), *want);
    }
}

#[test]
fn scenario_2_golden_range_stamped_names() {
    assert_eq!(
        naming::bin_name("bar_api", &PackageKind::Transform { from: 3, to: 4 }),
        "bar_api-transform-3-4"
    );
    assert_eq!(
        naming::bin_name("bar_api", &PackageKind::Engine { from: 1, to: 2 }),
        "bar_api-engine-1-2"
    );
}

#[test]
fn scenario_2_golden_symbol_prefix() {
    assert_eq!(naming::symbol_prefix("foo_services_blog"), "foo_services_blog_");
    assert_eq!(naming::symbol_prefix("bar-api"), "bar_api_");
}

#[test]
fn scenario_1_a_digit_leading_path_is_rescued() {
    let project = paths(&["2024-orders/schema.forge"]);
    let name = naming::app_name("", Path::new("2024-orders/schema.forge"), &project, SymbolNaming::Minimal);
    assert!(
        name.starts_with(|c: char| c.is_ascii_alphabetic()),
        "derived name must start with a letter, got {name:?}"
    );
    assert_eq!(name, "app_2024_orders");
}

#[test]
fn scenario_1_a_digit_leading_project_id_is_rescued() {
    let project = paths(&["schema.forge"]);
    let name = naming::app_name("2024", Path::new("schema.forge"), &project, SymbolNaming::Minimal);
    assert!(name.starts_with(|c: char| c.is_ascii_alphabetic()), "got {name:?}");
}

#[test]
fn scenario_1_an_empty_derivation_falls_back() {
    let project = paths(&["schema.forge"]);
    assert_eq!(
        naming::app_name("", Path::new("schema.forge"), &project, SymbolNaming::Minimal),
        "app"
    );
}

#[test]
fn scenario_1_every_kind_is_cargo_legal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = naming::app_name(
        "2024",
        Path::new("9lives/schema.forge"),
        &paths(&["9lives/schema.forge"]),
        SymbolNaming::Minimal,
    );

    let kinds = [
        PackageKind::Core,
        PackageKind::Server,
        PackageKind::Napi,
        PackageKind::Pyo3,
        PackageKind::Ffi,
        PackageKind::Wasm,
        PackageKind::Transform { from: 0, to: 1 },
        PackageKind::Engine { from: 9, to: 10 },
    ];

    for kind in &kinds {
        let name = naming::package_name(&app, kind);
        let crate_dir = dir.path().join(kind.dir());
        std::fs::create_dir_all(crate_dir.join("src")).expect("mkdir");
        std::fs::write(crate_dir.join("src/lib.rs"), "").expect("write lib");
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
        )
        .expect("write manifest");

        let out = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(&crate_dir)
            .output()
            .expect("run cargo metadata");
        assert!(
            out.status.success(),
            "cargo rejected the derived package name {name:?}:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn the_conventional_stem_is_dropped_and_others_are_kept() {
    assert_eq!(
        naming::app_segments(Path::new("services/blog/schema.forge")),
        vec!["services".to_string(), "blog".to_string()]
    );
    assert_eq!(
        naming::app_segments(Path::new("services/blog/api.forge")),
        vec!["services".to_string(), "blog".to_string(), "api".to_string()]
    );
}

#[test]
fn no_two_apps_in_a_project_share_a_name() {
    let project = paths(&[
        "services/blog/schema.forge",
        "services/cart/schema.forge",
        "app/blog/schema.forge",
        "app/blog/api.forge",
        "app/blog/api/schema.forge",
        "schema.forge",
        "deeply/nested/thing/schema.forge",
    ]);

    for mode in [SymbolNaming::Minimal, SymbolNaming::Uniform] {
        let mut seen: Vec<String> = project
            .iter()
            .map(|p| naming::app_name("proj", p, &project, mode))
            .collect();
        let total = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen.len(),
            total,
            "{mode:?} produced a duplicate app name across {total} apps: {seen:?}"
        );
    }
}

#[test]
fn every_segment_is_sanitised() {
    let rel = Path::new("my services/v1.2/order-book.forge");
    let name = naming::app_name("my proj", rel, &paths(&["my services/v1.2/order-book.forge"]), SymbolNaming::Uniform);
    assert!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "derived name must be a legal C identifier, got {name:?}"
    );
    assert_eq!(name, "my_proj_my_services_v1_2_order_book");
}
