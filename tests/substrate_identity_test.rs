use std::path::PathBuf;

use forgedb_source_guard::RustSource;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn substrate_sources() -> Vec<(String, RustSource)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("crates"))
        .expect("read crates/")
        .flatten()
    {
        let krate = entry.file_name().to_string_lossy().to_string();
        if krate == "codegen" {
            continue;
        }
        for src in RustSource::walk(entry.path().join("src"), &[]) {
            out.push((krate.clone(), src));
        }
    }
    assert!(
        out.len() > 20,
        "walked only {} substrate source files; the assertions below would hold over nothing",
        out.len()
    );
    out
}

#[test]
fn no_substrate_crate_reads_auto_sequences() {
    let mut violations = Vec::new();
    for (krate, src) in substrate_sources() {
        let reads = src.file_field_read_count("auto_sequences");
        if reads > 0 {
            violations.push(format!("{krate}: {} ({reads} read(s))", src.origin()));
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
    for (_, src) in substrate_sources() {
        if !src.structs_declaring_field("auto_sequences").is_empty() {
            found.push(
                PathBuf::from(src.origin())
                    .strip_prefix(&crates_dir)
                    .expect("a walked file lives under crates/")
                    .to_path_buf(),
            );
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
