//! Guards that `Manifest.auto_sequences` (#187) stays **inert** in the substrate.
//!
//! # Why a grep is the right shape here
//!
//! The generator-identity rule this defends is not "the field must not exist" —
//! it does exist, in two published crates. It is "no substrate crate may read,
//! interpret, order, or branch on it." That is a statement about *absence*, and
//! absence is exactly what a type system cannot express: adding
//! `impl Manifest { fn next_sequence(&mut self, field: &str) -> u64 }` compiles
//! cleanly, passes every test, and quietly turns a schema-agnostic byte-carrier
//! into a substrate that knows what an auto-increment is.
//!
//! Today the invariant is asserted only in a doc comment, which is a claim, not a
//! check. This file makes it a check.
//!
//! # What "inert" means, precisely
//!
//! The keys are `.forge` field names, so the field is not schema-*blind* in the
//! sense of carrying no schema-derived string — `ColumnMetadata.name` has carried
//! field names since #57, and that is fine for the same reason. What makes it
//! class-1 substrate is that the storage crates only serde it: they never *look
//! at* a key or a value. So the thing to forbid is a **field access**
//! (`.auto_sequences`), not a mention. A struct literal (`auto_sequences: …`),
//! the declaration (`pub auto_sequences: …`), and doc prose are all fine; a read
//! is not.
//!
//! `crates/codegen` is exempt by construction: it does not *read* the field, it
//! *emits the generated code that does*. That code is the schema-tailored owner
//! of the rule, which is the whole point.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under a directory.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `(crate_name, source_file)` for every crate in `crates/` except `codegen`.
fn substrate_sources() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("crates"))
        .expect("read crates/")
        .flatten()
    {
        let krate = entry.file_name().to_string_lossy().to_string();
        // The generator emits the code that owns the rule; it is not substrate.
        if krate == "codegen" {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&entry.path().join("src"), &mut files);
        out.extend(files.into_iter().map(|f| (krate.clone(), f)));
    }
    out
}

/// No crate outside `codegen` may READ `auto_sequences`.
///
/// A read is the violation, because a read is the first thing an interpretation
/// needs. Storing and returning the map is the sanctioned behaviour.
#[test]
fn no_substrate_crate_reads_auto_sequences() {
    let mut violations = Vec::new();
    for (krate, file) in substrate_sources() {
        let src = std::fs::read_to_string(&file).unwrap_or_default();
        for (i, line) in src.lines().enumerate() {
            // Skip doc/line comments: prose naming the field is how the
            // invariant is documented, and must not trip its own guard.
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains(".auto_sequences") {
                violations.push(format!("{krate}: {}:{}", file.display(), i + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "a substrate crate now READS `Manifest.auto_sequences`, which makes it \
         interpret a schema concept (#187 generator identity). Every read and \
         write of this map belongs to generated code:\n  {}",
        violations.join("\n  ")
    );
}

/// The field is declared in exactly the two backends and nowhere else.
///
/// Catches the other drift direction: a second substrate type growing its own
/// sequence state, which the read-guard above would not see.
#[test]
fn auto_sequences_is_declared_only_by_the_two_manifest_backends() {
    let expected = [
        PathBuf::from("storage-native/src/lib.rs"),
        PathBuf::from("storage-web/src/manifest.rs"),
    ];
    let crates_dir = repo_root().join("crates");

    let mut found = Vec::new();
    for (_, file) in substrate_sources() {
        let src = std::fs::read_to_string(&file).unwrap_or_default();
        if src.lines().any(|l| l.contains("pub auto_sequences")) {
            found.push(file.strip_prefix(&crates_dir).unwrap().to_path_buf());
        }
    }
    found.sort();

    assert_eq!(
        found,
        expected,
        "the set of substrate types declaring `auto_sequences` changed. The two \
         `Manifest` backends are the sanctioned holders (one per target, never \
         both in a build); a third declaration means sequence state has spread."
    );
}
