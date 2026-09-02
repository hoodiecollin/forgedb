use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn forgedb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".forgedb-home"))
        .args(args)
        .output()
        .expect("run forgedb")
}

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

    for present in ["schema.forge", "forgedb.toml", ".gitignore", "README.md"] {
        assert!(
            project.join(present).exists(),
            "init did not create {present}"
        );
    }
    assert!(project.join("generated").is_dir(), "no generated/ directory");
}

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

#[test]
fn the_dockerfile_drives_the_cli_and_copies_the_reported_artifact() {
    let (_tmp, project) = scaffold("myapp");
    let dockerfile = read(&project.join("Dockerfile"));

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
    assert!(
        dockerfile.contains(&format!("--version {}", env!("CARGO_PKG_VERSION"))),
        "the Dockerfile pins a version other than {}:\n{dockerfile}",
        env!("CARGO_PKG_VERSION")
    );

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

    assert!(
        dockerfile.contains("ENV FORGEDB_HOME=/forgedb"),
        "the builder stage does not name the build cache explicitly:\n{dockerfile}"
    );
    assert!(
        dockerfile.contains("FORGEDB_DATA=/data"),
        "the runtime data root is not an absolute path outside the cache:\n{dockerfile}"
    );

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

#[test]
fn the_gitignore_no_longer_ignores_generated_wholesale() {
    let (_tmp, project) = scaffold("myapp");
    let gitignore = read(&project.join(".gitignore"));

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

    for stale in ["/generated/**/*.a", "/generated/**/*.lib"] {
        assert!(
            !gitignore.contains(stale),
            "the root .gitignore still carries {stale}; it moved into \
             <output>/.gitignore (#337):\n{gitignore}"
        );
    }

    assert!(
        gitignore.lines().any(|l| l.trim() == "/data/"),
        ".gitignore stopped ignoring the data directory:\n{gitignore}"
    );
}

#[test]
fn s17_init_mints_a_unique_committed_id() {
    let (_a, one) = scaffold("api");
    let (_b, two) = scaffold("api");

    let read_id = |p: &std::path::Path| -> String {
        read(&p.join("forgedb.toml"))
            .lines()
            .find(|l| l.starts_with("id = "))
            .unwrap_or_else(|| panic!("no `id` in the scaffolded config of {}", p.display()))
            .to_string()
    };

    let (a, b) = (read_id(&one), read_id(&two));
    assert!(a.starts_with("id = \"api-"), "the slug keeps it legible: {a}");
    assert_ne!(a, b, "two scaffolds of the same directory name share an id");

    assert!(
        read(&one.join(".gitignore")).lines().all(|l| l.trim() != "forgedb.toml"),
        "the scaffold must not gitignore the file carrying the id"
    );
}

#[test]
fn s338_14_the_scaffolded_config_parses_and_carries_no_placement() {
    let (_tmp, project) = scaffold("myapp");
    let body = read(&project.join("forgedb.toml"));

    let config = forgedb::config::parse_config(&body, &project.join("forgedb.toml"))
        .unwrap_or_else(|e| panic!("the scaffolded config does not parse: {e}\n{body}"));

    assert!(
        config.placement.rust_package.is_none(),
        "the scaffold declared a placement; a new project is opted out by absence"
    );
    assert!(
        !body.contains("[placement]"),
        "the scaffold wrote a [placement] table:\n{body}"
    );
}
