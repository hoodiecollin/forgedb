use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const LINEAGE_VERSION: u32 = 7;

const CONFIGURED_FSYNC: &str = "Never";

const SCHEMA: &str = "User {\n  id: +uuid\n  email: string\n}\n";

fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir);
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd
}

fn scaffold(root: &Path) {
    fs::write(root.join("schema.forge"), SCHEMA).expect("write schema");
    fs::write(
        root.join("forgedb.toml"),
        "[project]\nid = \"dev-scenario-35\"\n\n\
         [generate]\ntargets = [\"rust\"]\n\n\
         [storage]\nfsync = \"never\"\n",
    )
    .expect("write config");

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

fn baked_fsync(generated_dir: &Path) -> Option<String> {
    const QUALIFIED: &str = "forgedb_wal::FsyncPolicy::";
    let src = fs::read_to_string(generated_dir.join("database.rs")).ok()?;
    let (_, rest) = src.split_once(QUALIFIED)?;
    Some(rest.chars().take_while(|c| c.is_alphanumeric()).collect())
}

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

fn baked(generated_dir: &Path) -> Option<(String, u32)> {
    Some((
        baked_fsync(generated_dir)?,
        baked_schema_version(generated_dir)?,
    ))
}

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

#[test]
fn dev_does_not_overwrite_a_correct_database_with_defaults() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    scaffold(root);

    let dev_out = root.join("dev-out");
    let logs = root.join("dev.log");
    let log = fs::File::create(&logs).expect("dev log");
    let errlog = log.try_clone().expect("clone dev log");

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
