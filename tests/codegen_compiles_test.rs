use std::path::PathBuf;

use forgedb::naming::PackageKind;

mod common;
use common::cache::Fixture;

const CONFIG: &str = r#"[project]
id = "codegen-compiles"
isolated = true

[generate]
targets = ["rust", "api"]
"#;

const SCHEMA: &str = r#"
enum Status { Draft, Published, Archived }

Author {
  id: +uuid
  email: &string
  name: ^string
  posts: [Post]
}

Post {
  @projection(card: title, status)
  id: +uuid
  title: ^string
  body: string
  summary: string?
  views: u32
  price: decimal
  status: ^Status
  meta: json
  author: *Author
  editor: ?Author
}
"#;

fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("codegen-check")
}

fn check_core_and_server(fx: &Fixture) -> std::process::Output {
    let core = fx.package_name(&PackageKind::Core);
    let server = fx.package_name(&PackageKind::Server);
    fx.cargo_in(&target_dir(), &["check", "-p", &core, "-p", &server])
}

#[test]
fn the_generated_core_and_server_type_check_against_the_checkout() {
    let fx = Fixture::generate(CONFIG, &[("schema.forge", SCHEMA)]);
    fx.patch_substrate();

    let out = check_core_and_server(&fx);
    assert!(
        out.status.success(),
        "the generated core and server packages do not type-check against the in-tree \
         substrate. The insta snapshots only compare strings, so this is the first place a \
         codegen change meets rustc on the PR gate:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_check_fails_when_the_emitted_core_does_not_compile() {
    let fx = Fixture::generate(CONFIG, &[("schema.forge", SCHEMA)]);
    fx.patch_substrate();

    let lib = fx.container().join("core").join("src").join("lib.rs");
    assert!(lib.is_file(), "no generated core at {}", lib.display());
    std::fs::write(&lib, "pub fn broken( {}\n").unwrap();

    let out = check_core_and_server(&fx);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "cargo check passed on a core package whose lib.rs is a syntax error, so the \
         positive test is not looking at the package it claims to:\n{stderr}"
    );
    let core = fx.package_name(&PackageKind::Core);
    assert!(
        stderr.contains(&core),
        "the failure does not name the core package `{core}`, so a real break would be \
         reported against something else:\n{stderr}"
    );
}
