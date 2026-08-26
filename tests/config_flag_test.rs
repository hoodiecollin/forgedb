//! End-to-end coverage for the global `--config` flag (#361).
//!
//! Nothing in the suite exercised `--config` before this file existed, which is
//! why `forgedb build` could ignore it for an entire release: `src/config.rs`'s
//! unit tests all call `toml::from_str` directly, so the *parsing* was covered
//! and the *loading* never was.
//!
//! These tests use a **three-valued** discriminator on purpose. With only two
//! states, a command that read no config at all and fell back to the built-in
//! default would be indistinguishable from one that read the right file:
//!
//! | source                    | `wal_checkpoint_interval` | `fsync`  |
//! |---------------------------|---------------------------|----------|
//! | built-in default          | 1000                      | `always` |
//! | `./forgedb.toml`          | 500                       | `never`  |
//! | the `--config` path       | 250                       | `always` |

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Built-in default, asserted against so a "read nothing" fix cannot pass.
const DEFAULT_INTERVAL: u64 = 1000;
/// What `./forgedb.toml` sets — the value #361 baked in erroneously.
const CWD_INTERVAL: u64 = 500;
/// What the `--config` file sets — the only correct answer.
const EXPLICIT_INTERVAL: u64 = 250;

fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir);
    // #333: `generate` claims this project id in the ForgeDB home. Both tests
    // here scaffold `name = "discriminator"` in a fresh tempdir, so without an
    // override the second one collides with the first's claim in the developer's
    // real `~/.forgedb` — and the collision refusal is correct, which is what
    // makes this a test-isolation bug rather than a product bug.
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

/// Run `build --plan` against `prod.toml` and assert it succeeded.
///
/// **`--plan` is load-bearing, not a speed-up applied to a test that did not
/// need one.** These tests are about *which config file `build` reads*, which is
/// settled in `main.rs` before cargo is ever invoked; `--plan` returns from
/// `build::run` (`src/commands/build/mod.rs:206-211`) after the code has been
/// emitted and the invocations constructed, but before `driver::execute`. Every
/// byte asserted on below is written on the way to that return, so the coverage
/// is unchanged and the release compile underneath it is not.
///
/// Without it these two tests cost ~173s: #335 moved the compile into the
/// ForgeDB-owned cache workspace, so the fixture's lack of a `Cargo.toml` — now
/// the *designed* state for a ForgeDB project, see `commands/init.rs` — stopped
/// making cargo fail cheaply and started making it succeed expensively (#380).
///
/// **Two things are asserted that a comment used to be trusted for**, because
/// #380 is precisely a comment quietly ceasing to be true:
///
/// 1. *The exit status.* The previous version declined to assert it, on the
///    stated grounds that the build failed for an unrelated reason — and when
///    that stopped being true nothing failed, because a comment cannot. The
///    assertion the stale premise talked us out of is the one that would have
///    caught the premise going stale.
/// 2. *That nothing was compiled.* `NOTHING_COMPILED` is `print_plan`'s last
///    line (`src/commands/build/mod.rs:445`), emitted only on the path that
///    returns before `driver::execute`. Asserting the **sentinel** rather than
///    trusting the flag is what stops this regressing the same way twice: if
///    `--plan` ever grows a compile step, this fails loudly instead of the suite
///    merely getting slow again with every assertion still green.
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

/// Read the `WAL_CHECKPOINT_INTERVAL` const out of a generated `database.rs`.
///
/// Parsed rather than substring-matched so `prettyplease` reformatting cannot
/// silently turn this guard into a no-op.
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

/// Read the baked `FsyncPolicy` variant out of a generated `database.rs`.
///
/// Matches only the **fully-qualified** `forgedb_wal::FsyncPolicy::` path. A doc
/// comment in the generated file contains a bare `` `FsyncPolicy::Always` ``
/// unconditionally, so a search for the unqualified name reports `Always` even
/// when `never` was configured — i.e. it passes while the bug is present.
fn baked_fsync(generated_dir: &Path) -> String {
    const PATH: &str = "forgedb_wal::FsyncPolicy::";
    let src = fs::read_to_string(generated_dir.join("database.rs"))
        .expect("generated database.rs should exist");
    let (_, rest) = src
        .split_once(PATH)
        .unwrap_or_else(|| panic!("no qualified {PATH} in {generated_dir:?}"));
    rest.chars().take_while(|c| c.is_alphanumeric()).collect()
}

/// `build` must bake the config the user named with `--config`, not the one that
/// happens to sit in the working directory.
///
/// `generate` runs first as an **in-run control**: it is known to thread the
/// loaded config through, so if it also came out wrong the failure would be in
/// config loading rather than in `build`, and the two diagnoses are very
/// different. Detection does not depend on the control; attribution does.
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

/// The same defect in its quieter direction: with no `forgedb.toml` in the
/// working directory the re-read silently fell back to built-in defaults, so the
/// user's production knobs simply did not apply and nothing was reported.
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
