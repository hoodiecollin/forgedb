use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GoFacts {
    pub import_paths: Vec<String>,
    pub import_aliases: std::collections::HashMap<String, String>,
    pub switch_tags: Vec<String>,
    pub string_switch_tags: Vec<String>,
    pub type_switches: usize,
    pub declared_types: Vec<String>,
    pub func_names: Vec<String>,
    pub decl_count: usize,
}

impl GoFacts {
    pub fn imports(&self, path: &str) -> bool {
        self.import_paths.iter().any(|p| p == path)
    }

    pub fn dispatches_generically(&self) -> bool {
        !self.string_switch_tags.is_empty() || self.type_switches > 0
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("source-guard lives at <root>/crates/source-guard")
        .to_path_buf()
}

fn goguard_bin() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let root = repo_root();
        let out = root.join("target").join("goguard").join("goguard");

        let status = Command::new("go")
            .args(["build", "-o"])
            .arg(&out)
            .arg(".")
            .current_dir(root.join("tools").join("goguard"))
            .status()
            .unwrap_or_else(|e| {
                panic!(
                    "source-guard: cannot run `go` ({e}).\n\
                     \n\
                     A Go toolchain is REQUIRED to run this test suite — it is not optional \
                     and this guard deliberately does not skip. A guard that skips reports \
                     green because it never evaluated, which is the exact failure this \
                     testkit exists to delete.\n\
                     \n\
                     Install Go (https://go.dev/dl/), or see docs/DEVELOPMENT.md."
                )
            });

        assert!(
            status.success(),
            "source-guard: `go build` of tools/goguard failed ({status}). \
             The helper is stdlib-only, so this is a toolchain problem, not a dependency one."
        );
        out
    })
}

pub fn go_facts(src: &str) -> GoFacts {
    use std::io::Write;

    let bin = goguard_bin();
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("source-guard: cannot spawn {}: {e}", bin.display()));

    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(src.as_bytes())
        .expect("write source to goguard");

    let out = child.wait_with_output().expect("wait for goguard");
    assert!(
        out.status.success(),
        "source-guard: goguard rejected the input — {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );

    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "source-guard: cannot decode goguard verdict: {e}\nraw: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}
