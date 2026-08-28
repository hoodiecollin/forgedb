use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const CONFIG: &str = "[project]\nid = \"removed-surface\"\n\n[generate]\ntargets = [\"rust\"]\n";
const SCHEMA: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";

fn forgedb(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".forgedb-home"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run forgedb {args:?}: {e}"));

    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "`forgedb {}` still succeeds — a removed surface must error, not no-op:\n{log}",
        args.join(" ")
    );
    for clap_said in ["unexpected argument", "unrecognized subcommand", "unknown flag"] {
        assert!(
            !log.contains(clap_said),
            "clap answered `forgedb {}` with {clap_said:?} — it names no replacement, so the \
             flag must survive as a tombstone whose behavior is the refusal:\n{log}",
            args.join(" ")
        );
    }
    log
}

fn project() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join("forgedb.toml"), CONFIG).unwrap();
    fs::write(tmp.path().join("schema.forge"), SCHEMA).unwrap();
    tmp
}

fn must_name(log: &str, args: &[&str], needles: &[&str]) {
    for needle in needles {
        assert!(
            log.contains(needle),
            "the diagnostic for `forgedb {}` does not name {needle:?}:\n{log}",
            args.join(" ")
        );
    }
}

#[test]
fn scenario_34_migrate_up() {
    let tmp = project();
    let invocations: [&[&str]; 2] = [
        &["migrate", "up"],
        &[
            "migrate", "up", "--from", "1", "--to", "2", "--src", "data", "--dest", "out",
        ],
    ];
    for args in invocations {
        let log = forgedb(tmp.path(), args);
        must_name(&log, args, &["migrate build", "migrate run", "#373"]);
    }
}

#[test]
fn scenario_34_build_target_wasm() {
    let tmp = project();
    let args: &[&str] = &["build", "--target", "wasm", "--schema", "schema.forge"];
    let log = forgedb(tmp.path(), args);
    must_name(&log, args, &["was removed", "[generate]", "browser-replica"]);

    let project_root = tmp.path().join(".forgedb-home/projects/removed-surface");
    assert!(
        !project_root.join("Cargo.toml").exists(),
        "`build --target wasm` rendered a cargo workspace on its way to refusing"
    );
    assert!(
        !project_root.join("target").exists(),
        "`build --target wasm` compiled something on its way to refusing"
    );
}

#[test]
fn scenario_34_removed_init_flags() {
    for flag in ["--rust", "--api-only"] {
        let tmp = TempDir::new().expect("tempdir");
        let args: &[&str] = &["init", "myapp", flag];
        let log = forgedb(tmp.path(), args);
        must_name(&log, args, &[flag, "removed", "targets", "forgedb.toml"]);
        assert!(
            !tmp.path().join("myapp").exists(),
            "`forgedb init myapp {flag}` scaffolded a directory before refusing"
        );
    }
}

#[test]
fn scenario_34_migrate_build_output() {
    let tmp = project();
    for flag in ["-o", "--output"] {
        let args: &[&str] = &[
            "migrate",
            "build",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
            flag,
            "somewhere",
        ];
        let log = forgedb(tmp.path(), args);
        must_name(&log, args, &["removed", "--schema"]);
        assert!(
            !tmp.path().join("somewhere").exists(),
            "`migrate build {flag}` emitted into the directory it was refusing:\n{log}"
        );
    }
}

#[test]
fn scenario_34_migrate_engine_output() {
    let tmp = project();
    let args: &[&str] = &[
        "migrate",
        "engine",
        "--schema",
        "schema.forge",
        "--src",
        "data",
        "--dest",
        "out",
        "-o",
        "somewhere",
    ];
    let log = forgedb(tmp.path(), args);
    must_name(&log, args, &["removed", "--schema"]);
    assert!(
        !tmp.path().join("somewhere").exists(),
        "`migrate engine -o` emitted into the directory it was refusing:\n{log}"
    );
}

#[test]
fn scenario_34_migrate_run_bin_dir() {
    let tmp = project();
    let args: &[&str] = &[
        "migrate",
        "run",
        "--schema",
        "schema.forge",
        "--from",
        "1",
        "--to",
        "2",
        "--src",
        "data",
        "--dest",
        "out",
        "--bin-dir",
        "migrations/transform",
    ];
    let log = forgedb(tmp.path(), args);
    must_name(&log, args, &["removed", "--from"]);
}

#[test]
fn scenario_34_migrate_create_auto() {
    let tmp = project();
    for flag in ["-a", "--auto"] {
        let args: &[&str] = &["migrate", "create", "some change", "--schema", "schema.forge", flag];
        let log = forgedb(tmp.path(), args);
        must_name(&log, args, &["removed", "--no-auto"]);
        assert!(
            !tmp.path().join("migrations").exists(),
            "`migrate create {flag}` recorded a migration while refusing the flag:\n{log}"
        );
    }
}

#[test]
fn scenario_34_every_refusal_site_has_a_row() {
    let migrate = include_str!("../src/commands/migrate/mod.rs");
    let migrate_calls = migrate.matches("refuse_removed_flag(").count() - 1;
    assert_eq!(
        migrate_calls, 4,
        "`migrate/mod.rs` has {migrate_calls} removed-flag refusals; this file covers 4 \
         (`migrate build -o`, `migrate engine -o`, `migrate run --bin-dir`, \
         `migrate create --auto`). Add a row."
    );
    assert!(
        migrate.contains("pub fn up()"),
        "the `migrate up` tombstone is gone — clap would answer it with \
         `unrecognized subcommand`, which names no replacement"
    );

    let build = include_str!("../src/commands/build/mod.rs");
    assert!(
        build.contains("reject_retired_target_flag(options.target.as_deref())?;"),
        "`build` no longer refuses `--target`, or refuses it somewhere other than first"
    );
}
