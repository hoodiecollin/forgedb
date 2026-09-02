use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const DEFINITION: &str = "crates/parser/src/ast.rs";

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

fn code_only(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let code = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(code);
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn offsets(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = haystack[from..].find(needle) {
        out.push(from + i);
        from += i + 1;
    }
    out
}

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
