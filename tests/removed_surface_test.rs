//! **Plan #347 scenario 34, in one place.** Every command and flag #335 removed
//! must *error naming its replacement* — never no-op, and never be answered by
//! clap.
//!
//! ```text
//! forgedb migrate up          forgedb build --target wasm
//! forgedb init --rust         forgedb init --api-only
//! forgedb migrate build -o    forgedb migrate run --bin-dir
//! ```
//!
//! # Why one file rather than three
//!
//! The scenario spans three steps of the plan — `build` (step 6), `migrate`
//! (step 8) and `init` (step 10) — and while it was being implemented it lived
//! as three thirds in three files, each unable to see the others. That is fine
//! for a work-in-progress and wrong to ship: the property is a **cross-cutting
//! rule about the CLI's whole removed surface**, and the failure mode it guards
//! against is *one* of them quietly being forgotten. Six cases in six places do
//! not add up to a rule; a table does, and a new removal is one row.
//!
//! # The two invariants every row carries
//!
//! 1. **Non-zero exit.** A removed flag that reads as applied and is not is the
//!    exact failure this whole issue deletes everywhere else.
//! 2. **ForgeDB answers, not clap.** Deleting the argument outright would hand
//!    the user `unexpected argument` / `unrecognized subcommand`, which names no
//!    replacement. So each removal survives as a *tombstone* that still parses
//!    and whose entire behavior is the refusal — and this file asserts clap's
//!    strings are absent, which is what proves the tombstone is doing the work.
//!
//! Per-row, each case also names the specific substrings the diagnostic must
//! carry, plus a side-condition asserting the command did nothing on its way to
//! refusing (no directory scaffolded, no output directory written, no cargo
//! cache created).

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const CONFIG: &str = "[project]\nname = \"removed-surface\"\n\n[generate]\ntargets = [\"rust\"]\n";
const SCHEMA: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";

/// Run the real binary with `FORGEDB_HOME` redirected into the fixture.
///
/// The redirect is not tidiness: without it these cases claim a project id and
/// write a build cache into the developer's real `~/.forgedb` (#333).
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
    // Asserted on every row rather than on a representative: a tombstone that
    // clap answers first is indistinguishable from a working one until someone
    // reads the message, and the whole point of a tombstone is the message.
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

/// A project a `migrate`/`build` row can be run against: a config and a schema,
/// and nothing else. Nothing here is generated, because every refusal under test
/// must fire *before* the command does any work.
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

// ---------------------------------------------------------------------------
// 1 · `forgedb migrate up`
// ---------------------------------------------------------------------------

/// `migrate up` was a wrapper over `migrate build` + `migrate run`, so the
/// refusal must name *both*, and say where the per-tenant sweep went (#373).
///
/// Run bare **and** spelled the way a runbook spells it. The second is the case
/// that matters: an operator does not type the naked subcommand, and a tombstone
/// clap answers first names no replacement.
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

// ---------------------------------------------------------------------------
// 2 · `forgedb build --target wasm`
// ---------------------------------------------------------------------------

/// `--target` was a cargo triple and invocation-wide, so `--target wasm` never
/// meant "also build the browser replica" — it meant "build EVERYTHING for this
/// triple". What replaced it is a *declaration*, so the refusal must name the
/// config table and the target value, not just say the flag is gone.
#[test]
fn scenario_34_build_target_wasm() {
    let tmp = project();
    let args: &[&str] = &["build", "--target", "wasm", "--schema", "schema.forge"];
    let log = forgedb(tmp.path(), args);
    must_name(&log, args, &["was removed", "[generate]", "browser-replica"]);

    // Nothing was BUILT. The bar is deliberately not "nothing happened":
    // `main.rs` resolves the project and reserves the app's cache container
    // before it dispatches to any command (`main.rs:706`/`:565`), so the
    // container and the `Build cache:` line exist by the time
    // `build::run`'s first statement — `reject_retired_target_flag` — is
    // reached. That reservation is an idempotent `mkdir` plus a `schema-path`
    // marker; it is not the class of side effect a tombstone must avoid, which
    // is one that makes the *retry* fail (see `init` below, where a
    // half-scaffolded directory would).
    //
    // What must not exist is anything the flag would have caused: a rendered
    // workspace root, or a `target/`.
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

// ---------------------------------------------------------------------------
// 3 + 4 · `forgedb init --rust` / `--api-only`
// ---------------------------------------------------------------------------

/// Both `init` flags selected a *generation* target, which is now declared in
/// `forgedb.toml`. The refusal must therefore name the flag, the key **and the
/// file the key lives in** — "use targets instead" is not actionable without it.
///
/// The side-condition is the sharp one: the refusal has to come before the
/// scaffolder touches the filesystem, or a half-written directory makes the
/// retry fail with "Project already exists" and buries the real diagnostic.
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

// ---------------------------------------------------------------------------
// 5 · `forgedb migrate build -o` / `--output`
// ---------------------------------------------------------------------------

/// The transformer is a member of the project's own cache workspace now, so an
/// operator-chosen output directory is not a location ForgeDB can honour: that
/// is how a generated `[package]` came to land under a foreign cargo root with
/// no `[workspace]` table (#328). Both spellings of the flag are checked —
/// removing only the long one leaves the short one silently accepted.
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

/// The same removal on `migrate engine` — the **mandatory** 0.4.0 upgrade
/// command, whose old default was `migrations/engine`: the exact path that
/// reproduces #328. It gets its own case for that reason rather than being
/// folded into the row above.
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

// ---------------------------------------------------------------------------
// 6 · `forgedb migrate run --bin-dir`
// ---------------------------------------------------------------------------

/// A directory cannot name a transformer any more: members are **range-stamped**
/// (`transform-<from>-<to>`), because one `transform/` per app collided across
/// ranges and `run` got whichever built last. So the refusal must name
/// `--from`/`--to`, which is what a directory alone cannot express.
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

// ---------------------------------------------------------------------------
// The rule, not the rows
// ---------------------------------------------------------------------------

/// Every removed surface is represented here, and the count is asserted so that
/// adding a removal without adding a row is a failure rather than a silence.
///
/// Anchored on the **refusal sites in the source**, not on a hand-kept number:
/// `refuse_removed_flag` in `migrate.rs` and `reject_retired_target_flag` /
/// the `init` refusal are the tokens that represent the work. A seventh removal
/// grows a seventh call site, and this test says so.
#[test]
fn scenario_34_every_refusal_site_has_a_row() {
    let migrate = include_str!("../src/commands/migrate.rs");
    // `refuse_removed_flag(` — one definition plus one call per removed flag.
    let migrate_calls = migrate.matches("refuse_removed_flag(").count() - 1;
    assert_eq!(
        migrate_calls, 3,
        "`migrate.rs` has {migrate_calls} removed-flag refusals; this file covers 3 \
         (`migrate build -o`, `migrate engine -o`, `migrate run --bin-dir`). Add a row."
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
