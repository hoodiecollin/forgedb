use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

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

fn substrate_sources() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("crates"))
        .expect("read crates/")
        .flatten()
    {
        let krate = entry.file_name().to_string_lossy().to_string();
        if krate == "codegen" {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&entry.path().join("src"), &mut files);
        out.extend(files.into_iter().map(|f| (krate.clone(), f)));
    }
    out
}

#[test]
fn no_substrate_crate_reads_auto_sequences() {
    let mut violations = Vec::new();
    for (krate, file) in substrate_sources() {
        let src = std::fs::read_to_string(&file).unwrap_or_default();
        for (i, line) in src.lines().enumerate() {
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
