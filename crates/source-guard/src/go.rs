//! The Go half — generated Go asserted through `go/parser`, across a process boundary.
//!
//! # Why a subprocess and not a Rust crate
//!
//! Go's standard library is the reference Go parser, and there is no Rust equivalent worth
//! hanging the identity red line on. `tree-sitter-go` yields a CST rather than an AST, does
//! no name resolution, and needs a C compiler — so it does not even remove a toolchain
//! dependency, it swaps Go for C. The one credible pure-Rust port is deprecated, ports Go
//! 1.12, and cannot parse generics.
//!
//! So: source in on stdin, JSON verdict out on stdout. No cgo, no `build.rs` dragging an
//! alien toolchain into cargo's dependency graph.
//!
//! # Missing Go is a hard failure, never a skip
//!
//! A guard that skips reports green because it never evaluated — the same class of failure
//! this whole crate exists to delete, and one this workspace has already been bitten by. If
//! the toolchain is absent, every Go guard fails loudly and says how to fix it.
//!
//! # Prebuilt, not `go run`
//!
//! `go run` recompiles on every invocation: ~300 ms versus ~3.75 ms for a prebuilt binary,
//! roughly 80×. The binary is built once per process, on first use, and cached on disk
//! under `target/`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// What `go/parser` saw. Mirrors `tools/goguard`'s `Facts` exactly.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GoFacts {
    /// Every imported package **path**, unquoted, alias discarded.
    ///
    /// This is the field to assert on. The original probe keyed its `reflect` detector on
    /// the identifier `reflect` and missed `import rt "reflect"` entirely — aliasing the
    /// import is the whole evasion. Match the path, never the local name.
    pub import_paths: Vec<String>,
    /// Local alias → path, for failure messages that need to explain the spelling.
    pub import_aliases: std::collections::HashMap<String, String>,
    /// Source text of every `switch <expr>` tag. A generic dispatcher shows up here
    /// whatever its variable is called — `switch kind` no more hides than `switch model`.
    pub switch_tags: Vec<String>,
    /// Only those switch tags whose cases include a **string literal**.
    ///
    /// This, not [`Self::switch_tags`], is what the identity red line asserts on. Generated
    /// Go legitimately switches on an integer status returned by the C ABI — nine such
    /// switches in a two-model schema — so banning all dispatch would ban the cgo wrappers.
    /// A generic model dispatcher must compare a model NAME, and a name is a string literal
    /// in the case clause. That shape has no legitimate reason to exist in per-model
    /// generated code, and it survives renaming both the variable and the import.
    pub string_switch_tags: Vec<String>,
    /// Count of `switch x := y.(type)` forms — generic dispatch with no string tag at all.
    pub type_switches: usize,
    pub declared_types: Vec<String>,
    pub func_names: Vec<String>,
    pub decl_count: usize,
}

impl GoFacts {
    /// Whether the file imports `path`, regardless of the local alias it was given.
    pub fn imports(&self, path: &str) -> bool {
        self.import_paths.iter().any(|p| p == path)
    }

    /// Whether the file dispatches the way a **generic model router** would: a switch whose
    /// cases are string literals, or a type switch.
    ///
    /// Deliberately NOT "has any switch". Generated Go contains legitimate integer status
    /// switches from the cgo wrappers (`switch r { case 1: …; case 0: … }`), and an
    /// invariant that fails on those is not the red line — it is noise that gets suppressed,
    /// which is how a red line stops being one.
    pub fn dispatches_generically(&self) -> bool {
        !self.string_switch_tags.is_empty() || self.type_switches > 0
    }
}

fn repo_root() -> PathBuf {
    // crates/source-guard -> crates -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("source-guard lives at <root>/crates/source-guard")
        .to_path_buf()
}

/// Build `tools/goguard` once per process and return the binary path.
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

/// Parse `src` as Go and return what `go/parser` saw.
///
/// # Panics
///
/// If the Go toolchain is missing, if the helper fails to build, or if `src` does not
/// parse. All three are hard failures by design — see the module docs.
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
