//! Scenario 35 (#335 gate 2 / #364): `dev` does not overwrite a correctly
//! generated database with defaults.
//!
//! # The defect
//!
//! `forgedb dev` never reached `commands::generate`.  It handed the schema path
//! straight to `forgedb_watcher::auto_watch`, whose `SchemaRegenerator` ran
//! `RustGenerator::generate` — which hardcodes `schema_version = 1` **and**
//! `GenConfig::DEFAULT`.  So on a project that configures `fsync = "never"` and
//! has a migration lineage, one save in `dev` replaced a correct `database.rs`
//! with a different database: `FsyncPolicy::Always`, and an open guard baked at
//! serial `1` that refuses the very data dir the app is running against.
//!
//! # Why the assertion is fully qualified
//!
//! `crates/codegen/src/rust.rs` emits **unconditional doc comments** containing
//! `` `FsyncPolicy::Always` ``.  A substring assertion on the unqualified name
//! therefore reports `Always` even when `never` was configured — it passes while
//! the bug is present.  Every assertion below matches
//! `forgedb_wal::FsyncPolicy::`, and [`the_bare_name_trap_is_still_there`]
//! proves the naive form would have been vacuous.
//!
//! # Why `dev` is run as a real subprocess
//!
//! `dev` blocks in the watch loop until Ctrl+C, so it cannot be called in
//! process.  The regeneration under test is the **initial** one `auto_watch`
//! performs before entering the loop; the test polls for its output, then kills
//! the child.  Nothing here mocks the watcher — mocking the component whose
//! behavior the bug misunderstood would encode the same wrong assumption.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// The lineage's highest `to_version`.  Deliberately not `1`: `1` is the
/// baseline an empty lineage yields *and* what the deleted watcher path
/// hardcoded, so the two would be indistinguishable.
const LINEAGE_VERSION: u32 = 7;

/// What `[storage] fsync` is set to.  The built-in default is `always`, so the
/// wrong answer and the "read no config" answer are the same value — which is
/// exactly the discriminator this scenario needs.
const CONFIGURED_FSYNC: &str = "Never";

const SCHEMA: &str = "User {\n  id: +uuid\n  email: string\n}\n";

fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir);
    // #333 claims this project id in the ForgeDB home; divert it so the test
    // neither collides with, nor writes into, the developer's real `~/.forgedb`.
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd
}

/// A project with a non-default durability knob **and** a migration lineage —
/// the two independent things the deleted path got wrong.
fn scaffold(root: &Path) {
    fs::write(root.join("schema.forge"), SCHEMA).expect("write schema");
    fs::write(
        root.join("forgedb.toml"),
        "[project]\nname = \"dev-scenario-35\"\n\n\
         [generate]\ntargets = [\"rust\"]\n\n\
         [storage]\nfsync = \"never\"\n",
    )
    .expect("write config");

    // A real lineage record, built by the crate that reads it back: the record
    // carries a checksum over its own fields, so a hand-written JSON blob would
    // be dropped at load time with a warning and the version would silently fall
    // back to the baseline — the test would then pass for the wrong reason.
    let migrations = root.join("migrations");
    fs::create_dir_all(&migrations).expect("migrations dir");
    let record = forgedb_migrations::Migration::new_versioned(
        "scenario 35 lineage".to_string(),
        Vec::new(),
        LINEAGE_VERSION - 1,
        LINEAGE_VERSION,
    );
    fs::write(
        migrations.join(record.filename()),
        serde_json::to_string_pretty(&record).expect("serialize migration"),
    )
    .expect("write migration");
}

/// The baked `FsyncPolicy` variant, matched only on the **fully-qualified** path.
fn baked_fsync(generated_dir: &Path) -> Option<String> {
    const QUALIFIED: &str = "forgedb_wal::FsyncPolicy::";
    let src = fs::read_to_string(generated_dir.join("database.rs")).ok()?;
    let (_, rest) = src.split_once(QUALIFIED)?;
    Some(rest.chars().take_while(|c| c.is_alphanumeric()).collect())
}

/// The baked `EXPECTED_SCHEMA_VERSION`.
///
/// Digits are taken from the **front** of the right-hand side, not filtered out
/// of it: `quote!` renders the literal as `7u32`, and a filter would read that
/// as `732`.
fn baked_schema_version(generated_dir: &Path) -> Option<u32> {
    let src = fs::read_to_string(generated_dir.join("database.rs")).ok()?;
    let line = src
        .lines()
        .find(|l| l.contains("const EXPECTED_SCHEMA_VERSION"))?;
    let rhs = line.rsplit('=').next()?.trim();
    rhs.chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// Both values, or `None` while the file is absent or half-written.
fn baked(generated_dir: &Path) -> Option<(String, u32)> {
    Some((
        baked_fsync(generated_dir)?,
        baked_schema_version(generated_dir)?,
    ))
}

// ---------------------------------------------------------------------------

/// **In-run control.** `generate` is known to thread the loaded config through,
/// so if it also came out wrong the fault would be in config loading rather than
/// in `dev` — and those are very different diagnoses.  Detection does not depend
/// on this; attribution does.
#[test]
fn generate_bakes_the_configured_policy_and_the_lineage_serial() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    scaffold(root);

    let out = forgedb_cmd(root)
        .args(["generate", "rust", "--output", "gen-control"])
        .output()
        .expect("run generate");
    assert!(
        out.status.success(),
        "control `generate` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (fsync, version) = baked(&root.join("gen-control")).expect("control emitted a database");
    assert_eq!(fsync, CONFIGURED_FSYNC, "`generate` must bake [storage] fsync");
    assert_eq!(
        version, LINEAGE_VERSION,
        "`generate` must bake the lineage-derived schema serial"
    );
}

/// **Scenario 35.** A `dev` regeneration emits the configured database, not the
/// default one.
#[test]
fn dev_does_not_overwrite_a_correct_database_with_defaults() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    scaffold(root);

    let dev_out = root.join("dev-out");
    let logs = root.join("dev.log");
    let log = fs::File::create(&logs).expect("dev log");
    let errlog = log.try_clone().expect("clone dev log");

    // `dev` blocks forever; the regeneration under test is the initial one
    // `auto_watch` runs before entering the watch loop. stdio goes to a file
    // rather than a pipe nobody drains, which would eventually block the child.
    let mut child = forgedb_cmd(root)
        .args(["dev", "--output", "dev-out"])
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errlog))
        .spawn()
        .expect("spawn dev");

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut observed = None;
    while Instant::now() < deadline {
        if let Some(pair) = baked(&dev_out) {
            observed = Some(pair);
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            panic!(
                "`dev` exited early ({status}) without emitting a database:\n{}",
                fs::read_to_string(&logs).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();

    let (fsync, version) = observed.unwrap_or_else(|| {
        panic!(
            "`dev` never emitted a readable {}:\n{}",
            dev_out.join("database.rs").display(),
            fs::read_to_string(&logs).unwrap_or_default()
        )
    });

    assert_eq!(
        fsync, CONFIGURED_FSYNC,
        "a `dev` regeneration baked `forgedb_wal::FsyncPolicy::{fsync}` — it read \
         no config and fell back to `GenConfig::DEFAULT` (#364)"
    );
    assert_eq!(
        version, LINEAGE_VERSION,
        "a `dev` regeneration baked EXPECTED_SCHEMA_VERSION = {version}; the \
         lineage says {LINEAGE_VERSION}. A database stamped at the baseline \
         refuses the data dir the running app is using (#364)"
    );
}

/// The trap the two assertions above are shaped around still exists, so the
/// qualification is load-bearing rather than decorative.
///
/// If this ever fails because the generator stopped emitting the bare name, the
/// correct response is to delete *this* test — never to relax the qualified
/// assertions, which are correct either way.
#[test]
fn the_bare_name_trap_is_still_there() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    scaffold(root);

    let out = forgedb_cmd(root)
        .args(["generate", "rust", "--output", "gen-trap"])
        .output()
        .expect("run generate");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let generated: PathBuf = root.join("gen-trap");
    let src = fs::read_to_string(generated.join("database.rs")).expect("database.rs");

    assert_eq!(
        baked_fsync(&generated).as_deref(),
        Some(CONFIGURED_FSYNC),
        "the configured policy is Never"
    );
    assert!(
        src.contains("FsyncPolicy::Always"),
        "the unqualified `FsyncPolicy::Always` no longer appears — a naive \
         substring assertion would now be a real guard, so this test has \
         outlived its purpose and should be deleted"
    );
    assert!(
        !src.contains("forgedb_wal::FsyncPolicy::Always"),
        "the QUALIFIED path must carry the configured policy only"
    );
}
