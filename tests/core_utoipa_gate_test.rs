use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = r#"
User {
  id: +uuid
  email: &string
  name: string
}
"#;

const CONFIG: &str = r#"[generate]
targets = ["all"]
"#;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    fn generate(target: &str) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&proj).unwrap();
        write(&proj.join("forgedb.toml"), CONFIG);
        write(&proj.join("schema.forge"), SCHEMA);

        let out = Command::new(env!("CARGO_BIN_EXE_forgedb"))
            .args(["generate", target, "--schema", "schema.forge"])
            .current_dir(&proj)
            .env("FORGEDB_HOME", &home)
            .output()
            .expect("run forgedb generate");
        assert!(
            out.status.success(),
            "`forgedb generate {target}` failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );

        Fixture { _tmp: tmp, home }
    }

    fn project_root(&self) -> PathBuf {
        let projects = self.home.join("projects");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&projects)
            .unwrap_or_else(|e| panic!("no cache at {}: {e}", projects.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), 1, "expected one project in the cache: {dirs:?}");
        dirs.pop().unwrap()
    }

    fn core_dir(&self) -> PathBuf {
        let apps = self.project_root().join("apps");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&apps)
            .unwrap_or_else(|e| panic!("no apps at {}: {e}", apps.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), 1, "expected one app container: {dirs:?}");
        dirs.pop().unwrap().join("core")
    }

    fn patch_substrate(&self) {
        let manifest = self.project_root().join("Cargo.toml");
        let mut body = read(&manifest);
        body.push_str("\n[patch.crates-io]\n");
        for dir in [
            "storage",
            "storage-native",
            "storage-web",
            "types",
            "changefeed",
            "wal",
            "compaction",
            "txn",
            "coordinator",
        ] {
            let path = repo_root().join("crates").join(dir);
            assert!(path.is_dir(), "no such substrate crate: {}", path.display());
            body.push_str(&format!(
                "forgedb-{dir} = {{ path = {:?} }}\n",
                path.to_string_lossy()
            ));
        }
        std::fs::write(&manifest, body).unwrap();
    }

    fn cargo_check(&self) -> std::process::Output {
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["check", "--workspace", "--target-dir"])
            .arg(self.home.join("cargo-target"))
            .current_dir(self.project_root())
            .env_remove("CARGO_TARGET_DIR")
            .output()
            .expect("cargo runs")
    }
}

fn pins_utoipa(manifest: &str) -> bool {
    manifest
        .lines()
        .any(|l| l.trim_start().starts_with("utoipa ") || l.trim_start().starts_with("utoipa="))
}

fn imports_utoipa(source: &str) -> bool {
    source.contains("use utoipa::")
}

#[test]
fn a_narrowing_generate_leaves_the_pin_and_the_import_agreeing() {
    let fx = Fixture::generate("rust");
    let core = fx.core_dir();
    let manifest = read(&core.join("Cargo.toml"));
    let source = read(&core.join("src/lib.rs"));

    assert_eq!(
        pins_utoipa(&manifest),
        imports_utoipa(&source),
        "core/Cargo.toml and core/src/lib.rs disagree about utoipa (#445).\n\
         pins = {}, imports = {}.\n\
         The emitted crate does not compile: `error[E0432]: unresolved import \
         `utoipa``.\n\
         Both sides must read GenConfig::needs_utoipa — see \
         crates/codegen/src/config.rs.\n\n\
         {manifest}",
        pins_utoipa(&manifest),
        imports_utoipa(&source),
    );
}

#[test]
fn the_fixture_really_does_declare_a_web_surface() {
    let fx = Fixture::generate("rust");
    let source = read(&fx.core_dir().join("src/lib.rs"));
    assert!(
        imports_utoipa(&source),
        "`targets = [\"all\"]` no longer makes the Rust core derive ToSchema — \
         this file's scenario has evaporated and the compile test below proves \
         nothing. Fix the fixture rather than deleting the assertion."
    );
}

#[test]
#[ignore = "compiles a generated crate with real cargo; tier 2 (`make test-ignored`)"]
fn the_core_a_narrowing_generate_writes_compiles() {
    let fx = Fixture::generate("rust");
    fx.patch_substrate();

    let out = fx.cargo_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the `core` package written by `forgedb generate rust` under \
         `targets = [\"all\"]` does not compile (#445).\n\n\
         core/Cargo.toml:\n{}\n\ncargo says:\n{stderr}",
        read(&fx.core_dir().join("Cargo.toml")),
    );
}
