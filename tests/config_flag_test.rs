use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const DEFAULT_INTERVAL: u64 = 1000;
const CWD_INTERVAL: u64 = 500;
const EXPLICIT_INTERVAL: u64 = 250;

fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir);
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd
}

fn write_schema(dir: &Path) {
    fs::write(dir.join("schema.forge"), "Post {\n  id: +uuid\n  title: string\n}\n")
        .expect("write schema");
}

fn write_config(path: &Path, interval: u64, fsync: &str) {
    fs::write(
        path,
        format!(
            "[project]\nid = \"discriminator\"\n\n\
             [storage]\nwal_checkpoint_interval = {interval}\nfsync = \"{fsync}\"\n"
        ),
    )
    .expect("write config");
}

fn build_cmd(root: &Path, output: &str) {
    const NOTHING_COMPILED: &str = "Nothing was compiled (--plan).";

    let out = forgedb_cmd(root)
        .args(["--config", "prod.toml", "build", "--plan", "--output", output])
        .output()
        .expect("run build");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "`build --plan` must exit 0: it compiles nothing, so the only ways it can \
         fail are the ones this test is about.\nstderr: {}\nstdout: {stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains(NOTHING_COMPILED),
        "`build --plan` did not report {NOTHING_COMPILED:?}, so this test is paying for \
         a release compile it asserts nothing about — the #380 regression.\nstdout: {stdout}"
    );
}

fn baked_interval(generated_dir: &Path) -> u64 {
    let src = fs::read_to_string(generated_dir.join("database.rs"))
        .expect("generated database.rs should exist");
    let line = src
        .lines()
        .find(|l| l.contains("const WAL_CHECKPOINT_INTERVAL"))
        .unwrap_or_else(|| panic!("no WAL_CHECKPOINT_INTERVAL const in {generated_dir:?}"));
    line.rsplit('=')
        .next()
        .expect("const has a value")
        .trim()
        .trim_end_matches(';')
        .parse()
        .unwrap_or_else(|_| panic!("unparseable const line: {line}"))
}

fn baked_fsync(generated_dir: &Path) -> String {
    const PATH: &str = "forgedb_wal::FsyncPolicy::";
    let src = fs::read_to_string(generated_dir.join("database.rs"))
        .expect("generated database.rs should exist");
    let (_, rest) = src
        .split_once(PATH)
        .unwrap_or_else(|| panic!("no qualified {PATH} in {generated_dir:?}"));
    rest.chars().take_while(|c| c.is_alphanumeric()).collect()
}

#[test]
fn build_honors_explicit_config_over_a_cwd_file() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    write_schema(root);
    write_config(&root.join("forgedb.toml"), CWD_INTERVAL, "never");
    write_config(&root.join("prod.toml"), EXPLICIT_INTERVAL, "always");

    let control = forgedb_cmd(root)
        .args(["--config", "prod.toml", "generate", "rust", "--output", "gen-generate"])
        .output()
        .expect("run generate");
    assert!(
        control.status.success(),
        "control `generate` failed: {}",
        String::from_utf8_lossy(&control.stderr)
    );
    assert_eq!(
        baked_interval(&root.join("gen-generate")),
        EXPLICIT_INTERVAL,
        "control: `generate` must read the --config file"
    );

    build_cmd(root, "gen-build");

    let built = root.join("gen-build");
    assert_eq!(
        baked_interval(&built),
        EXPLICIT_INTERVAL,
        "`build` baked the wrong config (#361): {} is ./forgedb.toml, {DEFAULT_INTERVAL} is the \
         built-in default, {EXPLICIT_INTERVAL} is the --config file the user named",
        CWD_INTERVAL
    );
    assert_eq!(
        baked_fsync(&built),
        "Always",
        "`build` baked a weaker durability policy than the named config asked for"
    );
}

#[test]
fn build_honors_explicit_config_when_no_cwd_file_exists() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    write_schema(root);
    write_config(&root.join("prod.toml"), EXPLICIT_INTERVAL, "always");
    assert!(!root.join("forgedb.toml").exists(), "fixture must have no CWD config");

    build_cmd(root, "gen-build");

    assert_eq!(
        baked_interval(&root.join("gen-build")),
        EXPLICIT_INTERVAL,
        "`build` fell back to defaults instead of reading --config"
    );
}
