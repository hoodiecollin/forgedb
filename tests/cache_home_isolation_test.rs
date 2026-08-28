use std::path::Path;

#[test]
fn every_binary_invoking_test_redirects_the_forgedb_home() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut offenders = Vec::new();

    let mut files: Vec<_> = walk(&tests);
    files.sort();

    for file in files {
        if file.file_name().and_then(|n| n.to_str()) == Some("cache_home_isolation_test.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&file).unwrap();
        if src.contains("CARGO_BIN_EXE_forgedb") && !src.contains("FORGEDB_HOME") {
            offenders.push(file.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "these tests run the forgedb binary without redirecting FORGEDB_HOME, so they \
         claim project ids in the developer's real ~/.forgedb:\n  {}\n\n\
         Set it on the Command: `.env(\"FORGEDB_HOME\", <tempdir>.join(\".forgedb-home\"))`.",
        offenders.join("\n  ")
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}
