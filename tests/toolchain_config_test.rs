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
