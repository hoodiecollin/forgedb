//! `[toolchain]` is strict, and it is checked EARLY (#374 direction C,
//! gate 1 decision 3).
//!
//! # The name is a one-way door
//!
//! Config parsing is `#[serde(deny_unknown_fields)]`, so **every already-released
//! `forgedb` rejects a config carrying a table it does not know**. A project
//! that adopts `[toolchain]` therefore cannot be built by an older CLI, and
//! renaming the table later would strand every config that had adopted the first
//! name. The spelling ships once. `the_table_is_spelled_toolchain` is a
//! deliberately dumb guard on exactly that.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const SCHEMA: &str = "Post {\n  id: +uuid\n  title: string\n}\n";

fn forgedb(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn project(config: &str) -> TempDir {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("forgedb.toml"), config).unwrap();
    fs::write(t.path().join("schema.forge"), SCHEMA).unwrap();
    t
}

const BASE: &str = "[project]\nid = \"toolchain-fixture\"\n\n[generate]\ntargets = [\"all\"]\n";

/// The table parses, with every key and every sub-key.
#[test]
fn the_table_parses_with_every_documented_key() {
    let t = project(&format!(
        "{BASE}\n[toolchain]\n\
         bun    = {{ path = \"/opt/homebrew/bin/bun\", min_version = \"1.1\" }}\n\
         node   = {{ path = \"/usr/local/bin/node\", min_version = \"20\" }}\n\
         python = {{ path = \".venv/bin/python\", min_version = \"3.11\" }}\n"
    ));
    let out = forgedb(t.path())
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`[toolchain]` must parse:\n{}",
        combined(&out)
    );
}

/// A misspelled key is REFUSED, not silently ignored.
///
/// This is why the table is three explicit fields and not a
/// `HashMap<String, InterpreterConfig>`: a map accepts every key even under
/// `deny_unknown_fields`, so `pythn = { ... }` would parse and be ignored — a
/// configuration that reads as applied and is not.
#[test]
fn a_misspelled_interpreter_is_refused() {
    for bad in ["pythn = { path = \"/x\" }", "deno = { path = \"/x\" }"] {
        let t = project(&format!("{BASE}\n[toolchain]\n{bad}\n"));
        let out = forgedb(t.path())
            .args(["migrate", "status", "--schema", "schema.forge"])
            .output()
            .unwrap();
        assert!(
            !out.status.success(),
            "`[toolchain] {bad}` must be refused:\n{}",
            combined(&out)
        );
    }
}

/// A misspelled SUB-key is refused too. `min_ver` reads as a version floor and
/// is not one.
#[test]
fn a_misspelled_sub_key_is_refused() {
    let t = project(&format!(
        "{BASE}\n[toolchain]\npython = {{ path = \"/x\", min_ver = \"3.11\" }}\n"
    ));
    let out = forgedb(t.path())
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{}", combined(&out));
}

/// The table is spelled `toolchain` — a one-way door, guarded on purpose.
///
/// `[runtime]` was taken (replication, change-feed capacity, cascade depth), so
/// this needed its own name; and under `deny_unknown_fields` the name cannot be
/// changed afterwards without breaking every config that adopted it. The guard
/// reads the DECLARATION in `config.rs` with comments stripped, so the prose
/// explaining the invariant cannot satisfy it.
#[test]
fn the_table_is_spelled_toolchain() {
    let src: String = include_str!("../src/config.rs")
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        src.contains("pub toolchain: ToolchainConfig,"),
        "`ForgeConfig::toolchain` was renamed. Under `deny_unknown_fields` every \
         already-released forgedb rejects an unknown table, so this spelling ships \
         once: a rename strands every config that adopted the first name."
    );
    assert!(
        !src.contains("pub runtime: ToolchainConfig"),
        "`[runtime]` already means replication / change-feed capacity / cascade \
         depth; overloading it is what the separate table exists to avoid"
    );
}
