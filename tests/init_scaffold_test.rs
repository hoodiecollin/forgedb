//! What `forgedb init` scaffolds after #335 — and, just as load-bearing, what it
//! no longer scaffolds.
//!
//! Plan #347 scenario **37**. Scenario 34's `init` rows (`--rust`,
//! `--api-only`) live in `tests/removed_surface_test.rs` with the other four:
//! the removed surface is one cross-cutting rule, and a rule split across the
//! three files that happened to implement it cannot notice a row going missing.
//!
//! Every assertion runs the real binary in a tempdir with `FORGEDB_HOME`
//! redirected (see `cache_home_isolation_test`), because the thing under test is
//! the *files on disk after the command*, and a unit test that called the
//! scaffolder directly would prove nothing about the clap surface that reaches
//! it.
//!
//! **Anchored on the work, not on labels.** The Dockerfile "drives the CLI" is
//! asserted as the presence of the `--print-artifact server` *invocation* and the
//! absence of any `cargo build` of a user crate — not on the word "forgedb"
//! appearing in a comment, which the old Dockerfile also managed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Run `forgedb <args...>` in `dir`, with the ForgeDB home redirected inside it.
fn forgedb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".forgedb-home"))
        .args(args)
        .output()
        .expect("run forgedb")
}

/// `forgedb init <name>` in a fresh tempdir; returns the tempdir and the project
/// directory. Panics with the captured output if the command failed, so a
/// regression reads as the CLI's own diagnostic rather than as a missing file.
fn scaffold(name: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let out = forgedb(tmp.path(), &["init", name]);
    assert!(
        out.status.success(),
        "forgedb init failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let project = tmp.path().join(name);
    assert!(project.is_dir(), "init created no project directory");
    (tmp, project)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Scenario 37 — `forgedb init` scaffolds no cargo package.
// ---------------------------------------------------------------------------

/// The headline: nothing in the scaffold is a cargo package, because the
/// generated Rust is compiled in ForgeDB's cache now (#335 §15).
#[test]
fn init_scaffolds_no_cargo_package() {
    let (_tmp, project) = scaffold("myapp");

    assert!(
        !project.join("Cargo.toml").exists(),
        "init scaffolded a Cargo.toml — the generated Rust lives in the ForgeDB \
         cache after #335, not in the user's repo"
    );
    assert!(
        !project.join("Cargo.lock").exists(),
        "init scaffolded a Cargo.lock"
    );
    assert!(
        !project.join("src").exists(),
        "init scaffolded src/ — that was the cargo package's source directory"
    );
    assert!(
        !project.join("src/main.rs").exists(),
        "init scaffolded a server main.rs"
    );

    // What it DOES scaffold, so "delete everything" cannot pass this test.
    for present in ["schema.forge", "forgedb.toml", ".gitignore", "README.md"] {
        assert!(
            project.join(present).exists(),
            "init did not create {present}"
        );
    }
    assert!(project.join("generated").is_dir(), "no generated/ directory");
}

/// The deploy files are present for EVERY project now — they used to be gated on
/// the Rust scaffold existing (`init.rs`'s `options.rust || !options.api_only`),
/// and that gate is gone with the scaffold.
#[test]
fn init_always_emits_the_deploy_files() {
    let (_tmp, project) = scaffold("myapp");

    for present in [
        "Dockerfile",
        ".dockerignore",
        "docker-compose.yml",
        "deploy/myapp.service",
        "deploy/myapp.env",
        "deploy/README.md",
    ] {
        assert!(
            project.join(present).exists(),
            "init did not create {present}"
        );
    }
}

/// The Dockerfile drives the CLI and copies out the path ForgeDB *reports*.
///
/// Anchored on the invocation, not on prose: the pre-#335 Dockerfile also said
/// "ForgeDB" in a comment while doing `RUN cargo build --release` on a scaffolded
/// crate that no longer exists.
#[test]
fn the_dockerfile_drives_the_cli_and_copies_the_reported_artifact() {
    let (_tmp, project) = scaffold("myapp");
    let dockerfile = read(&project.join("Dockerfile"));

    // It resolves the artifact rather than constructing a path.
    assert!(
        dockerfile.contains("--print-artifact server"),
        "the Dockerfile does not ask ForgeDB where the server binary is:\n{dockerfile}"
    );
    assert!(
        dockerfile.contains("$(forgedb build"),
        "the artifact path is not command-substituted from `forgedb build`:\n{dockerfile}"
    );
    assert!(
        dockerfile.contains("forgedb generate"),
        "the Dockerfile never generates:\n{dockerfile}"
    );
    assert!(
        dockerfile.contains("cargo install forgedb --version"),
        "the Dockerfile does not install a PINNED forgedb:\n{dockerfile}"
    );
    // The pin is this CLI's own version — a scaffold that pinned some other
    // release would generate different code than the CLI that wrote it.
    assert!(
        dockerfile.contains(&format!("--version {}", env!("CARGO_PKG_VERSION"))),
        "the Dockerfile pins a version other than {}:\n{dockerfile}",
        env!("CARGO_PKG_VERSION")
    );

    // Every input of the old builder stage is gone.
    for dead in [
        "COPY Cargo.toml",
        "COPY src ./src",
        "cargo build --release",
        "/build/target/release/",
    ] {
        assert!(
            !dockerfile.contains(dead),
            "the Dockerfile still carries the pre-#335 cargo build path {dead:?}:\n{dockerfile}"
        );
    }

    // CONTRACT-artifact-report §6, properties 1 and 2.
    assert!(
        dockerfile.contains("ENV FORGEDB_HOME=/forgedb"),
        "the builder stage does not name the build cache explicitly:\n{dockerfile}"
    );
    assert!(
        dockerfile.contains("FORGEDB_DATA=/data"),
        "the runtime data root is not an absolute path outside the cache:\n{dockerfile}"
    );

    // Property 3: `--print-artifact` is handed the stable KIND, never a package
    // name — package names carry the per-app hash and change when the schema file
    // is renamed. Asserted by reading the token that follows the flag.
    let invocation = dockerfile
        .lines()
        .find(|l| !l.trim_start().starts_with('#') && l.contains("$(forgedb build"))
        .unwrap_or_else(|| panic!("no `$(forgedb build …)` command line:\n{dockerfile}"));
    let kind = invocation
        .split("--print-artifact ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|k| k.trim_end_matches(|c| c == ')' || c == '"').to_string())
        .expect("--print-artifact has an argument");
    assert_eq!(
        kind, "server",
        "the Dockerfile selects {kind:?} rather than the stable `server` kind"
    );
}

/// The on-host path is the container path's sibling and must not have been left
/// behind on `cargo build` either.
#[test]
fn the_systemd_readme_installs_the_reported_artifact() {
    let (_tmp, project) = scaffold("myapp");
    let readme = read(&project.join("deploy/README.md"));

    assert!(
        readme.contains("--print-artifact server"),
        "deploy/README.md does not install the artifact ForgeDB reports:\n{readme}"
    );
    assert!(
        !readme.contains("target/release/myapp"),
        "deploy/README.md still installs from a hand-constructed target path:\n{readme}"
    );
}

/// `.gitignore` stops ignoring `/generated/` wholesale (#335 §15): generated text
/// is committed, only compiled output is ignored.
#[test]
fn the_gitignore_no_longer_ignores_generated_wholesale() {
    let (_tmp, project) = scaffold("myapp");
    let gitignore = read(&project.join(".gitignore"));

    // A wholesale ignore is a bare `/generated/` (or `generated/`) PATTERN line —
    // matched as a line rather than a substring so the narrow rules below, which
    // contain the same characters, cannot make this pass vacuously.
    let wholesale: Vec<&str> = gitignore
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter(|l| *l == "/generated/" || *l == "generated/" || *l == "/generated" || *l == "generated")
        .collect();
    assert!(
        wholesale.is_empty(),
        ".gitignore still ignores generated code wholesale ({wholesale:?}):\n{gitignore}"
    );

    // ...and it still ignores the one compiled artifact delivered into the tree
    // (design #335 §6: the Go binding links `output/go/libforgedb.a`).
    assert!(
        gitignore.contains("/generated/**/*.a"),
        ".gitignore does not ignore the delivered static library:\n{gitignore}"
    );

    // Data is still ignored — this is not "ignore nothing".
    assert!(
        gitignore.lines().any(|l| l.trim() == "/data/"),
        ".gitignore stopped ignoring the data directory:\n{gitignore}"
    );
}
