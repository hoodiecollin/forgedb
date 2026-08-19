//! Every test that runs the `forgedb` binary must redirect the ForgeDB home.
//!
//! Since #333, `forgedb generate` claims its project id in a ledger under
//! `$FORGEDB_HOME`, defaulting to `~/.forgedb`. A test that does not override it
//! writes into the developer's real home, and two fixtures that happen to share a
//! `[project].name` then collide **across unrelated test runs** — the second one
//! failing for a reason that is nowhere in its own source. That is exactly what
//! happened to `config_flag_test`, whose two tests both scaffold
//! `name = "discriminator"`.
//!
//! This guard is anchored on `CARGO_BIN_EXE_forgedb` — the token that represents
//! *running the binary*, which is the thing that can pollute — rather than on a
//! helper's name, which a new file is free not to use.

use std::path::Path;

#[test]
fn every_binary_invoking_test_redirects_the_forgedb_home() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut offenders = Vec::new();

    let mut files: Vec<_> = walk(&tests);
    files.sort();

    for file in files {
        // This file names the constant while describing it, and runs nothing.
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
