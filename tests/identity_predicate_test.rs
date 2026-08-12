//! **#251 scenario 12.** No open-coded copy of the identity predicate survives.
//!
//! # Why a grep is the right shape here
//!
//! `f.name == "id" || f.auto_generate` was open-coded **37 times across 9 files**
//! — 14 of them `find()`ing the field (those *select*, so precedence changes
//! their answer) and the rest `any()`ing it (existence only). A partial fix is
//! worse than none: if `validate.rs` selects a different field than `rust.rs`
//! keys on, the guard checks a field the generator does not use, and the schema
//! is validated against a key the database does not have.
//!
//! The invariant #251 commits to is therefore *absence*, and absence is what no
//! type system expresses: re-open-coding the disjunction at a new site compiles
//! cleanly, passes every test, and silently reintroduces first-match ordering at
//! exactly one place. Only a scan over the sources can say "there is one
//! definition."
//!
//! # What is forbidden, precisely
//!
//! The **disjunction** — `<binding>.name == "id" || <binding>.auto_generate`, in
//! either order. Not the mention of either half:
//!
//! - `find(|f| f.name == "id")` alone is fine (a test looking up a field by name).
//! - `f.name == "id" && f.auto_generate && …` is fine and deliberate — that is
//!   `RustGenerator::timestamp_key_field` (#254), a *narrower* question ("is this
//!   the allocated timestamp key?"), not a second copy of "which field is the
//!   identity?".
//! - Prose and doc comments are stripped before scanning, so explaining the old
//!   shape in a comment stays legal.
//!
//! `crates/parser/src/ast.rs` is exempt: it holds the one definition.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The single file allowed to spell the predicate out.
const DEFINITION: &str = "crates/parser/src/ast.rs";

/// This file, which necessarily contains the forbidden text as its own needle.
const SELF: &str = "tests/identity_predicate_test.rs";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target" || n == "snapshots") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Drop `//` comments (including doc comments) and collapse runs of whitespace,
/// so the scan sees code and is insensitive to rustfmt's line breaking.
fn code_only(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let code = match line.find("//") {
            // Not perfect (a `//` inside a string literal would truncate), but the
            // predicate never appears inside one, and erring toward *less* text
            // can only produce false negatives in prose — never a false positive.
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(code);
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every byte offset at which `needle` occurs in `haystack`.
fn offsets(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = haystack[from..].find(needle) {
        out.push(from + i);
        from += i + 1;
    }
    out
}

/// The forbidden shape: a name-is-`id` test **or**-ed with an auto-generate test.
/// The two halves sit within a few tokens of each other in every copy that ever
/// existed; the window is generous enough to survive a rename of the binding and
/// tight enough that the shared definition's two *separate* `find`s (about 70
/// characters apart, and in `ast.rs` anyway) cannot trip it.
fn open_codes_the_disjunction(code: &str) -> Vec<String> {
    const WINDOW: usize = 32;
    let mut hits = Vec::new();

    for (anchor, other) in [("== \"id\" ||", "auto_generate"), ("auto_generate ||", "== \"id\"")] {
        for i in offsets(code, anchor) {
            let end = (i + WINDOW).min(code.len());
            if code[i..end].contains(other) {
                hits.push(code[i..end].to_string());
            }
        }
    }
    hits
}

#[test]
fn the_identity_predicate_has_exactly_one_definition() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);
    files.sort();
    assert!(files.len() > 50, "expected the whole workspace, found {}", files.len());

    let mut offenders = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        if rel == DEFINITION || rel == SELF {
            continue;
        }
        let src = std::fs::read_to_string(path).expect("read source");
        let code = code_only(&src);
        for snippet in open_codes_the_disjunction(&code) {
            offenders.push(format!("{rel}: …{snippet}…"));
        }
        // A *re-derived* helper is the same defect wearing the right name: the
        // four two-pass `fn identity_field` copies #254 left behind agreed with
        // each other only by having been written on the same afternoon. Nothing
        // stops the fifth from drifting, and the disjunction scan above cannot
        // see it because the two-pass spelling never `||`s the halves together.
        if code.contains("fn identity_field(") {
            offenders.push(format!("{rel}: re-defines `fn identity_field` — call the AST's"));
        }
    }

    assert!(
        offenders.is_empty(),
        "the identity predicate is open-coded again — route these through \
         `Model::identity_field()` / `Model::has_identity()` in {DEFINITION}:\n{}",
        offenders.join("\n")
    );
}

/// The mirror assertion: the definition must actually BE in `ast.rs`. Without
/// this, deleting the shared helper and every call site would pass the test
/// above — an invariant that a vacuum satisfies is not an invariant.
#[test]
fn the_one_definition_is_on_the_ast() {
    let src = std::fs::read_to_string(repo_root().join(DEFINITION)).expect("read ast.rs");
    let code = code_only(&src);
    assert!(
        code.contains("pub fn identity_field(&self)"),
        "`Model::identity_field` is the shared definition and it must live in {DEFINITION}"
    );
    assert!(
        code.contains("pub fn has_identity(&self)"),
        "`Model::has_identity` must be derived from it, not open-coded"
    );
}
