use std::path::PathBuf;

use forgedb_source_guard::RustSource;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const DEFINITION: &str = "crates/parser/src/ast.rs";

const SELF: &str = "tests/identity_predicate_test.rs";

fn workspace_sources() -> Vec<RustSource> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["crates", "src", "tests"] {
        files.extend(RustSource::walk(root.join(dir), &["target", "snapshots"]));
    }
    assert!(files.len() > 50, "expected the whole workspace, found {}", files.len());
    files
}

fn rel(src: &RustSource) -> String {
    PathBuf::from(src.origin())
        .strip_prefix(repo_root())
        .expect("a walked file lives under the repo root")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn the_identity_predicate_has_exactly_one_definition() {
    let mut offenders = Vec::new();
    for src in workspace_sources() {
        let rel = rel(&src);
        if rel == DEFINITION || rel == SELF {
            continue;
        }
        for expr in src.or_expressions_joining("id", "auto_generate") {
            offenders.push(format!("{rel}: {expr}"));
        }
        if src.free_fn_names().iter().any(|f| f == "identity_field")
            || src.methods_named("identity_field").is_ok()
        {
            offenders.push(format!("{rel}: re-defines `fn identity_field`; call the AST's"));
        }
    }

    assert!(
        offenders.is_empty(),
        "the identity predicate is open-coded again. Route these through \
         `Model::identity_field()` / `Model::has_identity()` in {DEFINITION}:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_one_definition_is_on_the_ast() {
    let src = RustSource::repo_file(repo_root().join(DEFINITION));
    src.method_in("Model", "identity_field").unwrap_or_else(|e| {
        panic!("`Model::identity_field` is the shared definition and it must live in {DEFINITION}: {e}")
    });
    src.method_in("Model", "has_identity").unwrap_or_else(|e| {
        panic!("`Model::has_identity` must be derived from it, not open-coded: {e}")
    });
}
