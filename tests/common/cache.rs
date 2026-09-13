use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use forgedb::naming;

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

pub struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    pub fn generate(config: &str, schemas: &[(&str, &str)]) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&proj).unwrap();
        write(&proj.join("forgedb.toml"), config);
        for (rel, body) in schemas {
            write(&proj.join(rel), body);
        }

        for (rel, _) in schemas {
            let out = Command::new(env!("CARGO_BIN_EXE_forgedb"))
                .args(["generate", "all", "--schema", rel])
                .current_dir(&proj)
                .env("FORGEDB_HOME", &home)
                .output()
                .expect("run forgedb generate");
            assert!(
                out.status.success(),
                "`forgedb generate all --schema {rel}` failed:\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }

        Fixture { _tmp: tmp, home }
    }

    pub fn project_root(&self) -> PathBuf {
        let projects = self.home.join("projects");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&projects)
            .unwrap_or_else(|e| panic!("no cache at {}: {e}", projects.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(
            dirs.len(),
            1,
            "expected exactly one project in the cache, found {dirs:?}"
        );
        dirs.pop().unwrap()
    }

    pub fn containers(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(self.project_root().join("apps"))
            .expect("apps/ exists")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs
    }

    pub fn container(&self) -> PathBuf {
        let mut c = self.containers();
        assert_eq!(c.len(), 1, "expected exactly one app container, got {c:?}");
        c.pop().unwrap()
    }

    pub fn package_name(&self, kind: &naming::PackageKind) -> String {
        let app_dir = self.container();
        let app = forgedb::cache::member_app_name(&app_dir).unwrap_or_else(|| {
            panic!(
                "no app-name marker in {}; `cache::reserve` writes one",
                app_dir.display()
            )
        });
        naming::package_name(&app, kind)
    }

    pub fn patch_substrate(&self) {
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
            "auth",
            "query-params",
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

    pub fn cargo(&self, args: &[&str]) -> Output {
        self.cargo_in(&self.target_dir(), args)
    }

    pub fn cargo_in(&self, target_dir: &Path, args: &[&str]) -> Output {
        let compiles = args.first().is_some_and(|a| *a == "build" || *a == "check");
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.args(args);
        if compiles {
            cmd.arg("--target-dir").arg(target_dir);
        }
        cmd.current_dir(self.project_root())
            .env_remove("CARGO_TARGET_DIR")
            .output()
            .expect("cargo runs")
    }

    pub fn target_dir(&self) -> PathBuf {
        self.home.join("cargo-target")
    }

    pub fn project_dir(&self) -> PathBuf {
        self._tmp.path().join("proj")
    }

    pub fn forgedb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_forgedb"))
            .args(args)
            .current_dir(self.project_dir())
            .env("FORGEDB_HOME", &self.home)
            .output()
            .expect("run forgedb")
    }
}
