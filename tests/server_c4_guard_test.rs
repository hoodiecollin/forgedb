//! **Plan #347 scenario 36.** The generated server refuses to create a database
//! inside the ForgeDB home, names the build cache, and tells the user what to
//! pass instead.
//!
//! # Why this population exists at all
//!
//! `TenantConfig::root()` defaults to a **relative** `data`. Since #335 the
//! server binary lives *only* in the build cache, `forgedb build` prints that
//! cache path (C7), and C3 expressly allows running a dev server out of it. So
//! `cd ~/.forgedb/projects/<id> && ./target/release/<app>-server` creates
//! `<that dir>/data/` and quietly turns a cache — a directory ForgeDB is free to
//! delete (C8) — into an installation. The people who hit this are exactly the
//! people following a path ForgeDB printed them.
//!
//! # Why the guard is COMPILED AND RUN here rather than string-asserted
//!
//! `server_pkg.rs` already has a unit test asserting the body *contains* the
//! phrase and the two helper names. That is a guard against deletion, not
//! against being wrong: the containment logic has three ways to be silently
//! useless — `$FORGEDB_HOME` resolved differently from the CLI, a purely lexical
//! `starts_with` defeated by macOS's `/tmp` → `/private/tmp` symlink, and a
//! `data` root that does not exist yet so nothing canonicalizes. None of those
//! change a single byte of the text.
//!
//! So this test extracts the REAL guard source out of the REAL generated
//! `main.rs` — the region from the env reads through the end of the `if let
//! Some(home)` block, plus the two helpers verbatim — compiles it with plain
//! `rustc` (no cargo, no deps: the guard is pure `std`), and runs the resulting
//! binary under a planted `FORGEDB_HOME`. Both outcomes are measured: a data
//! root inside the home must exit non-zero with the message, and one outside
//! must reach the line after the guard.
//!
//! Extraction is anchored on the two statements that bracket the guard, and the
//! test fails loudly if either anchor moves — a silently-empty extraction would
//! be a test that compiles nothing and passes.

use forgedb_codegen::ServerPackage;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// The statement the guard region begins at, and the one it ends before.
const REGION_START: &str = "    let tenant = std::env::var(\"FORGEDB_TENANT\").ok();";
const REGION_END: &str = "    let db = std::sync::Arc::new(";

/// A top-level `fn` lifted verbatim: from its `fn <name>` line to the next
/// line that is exactly `}`.
fn lift_fn(source: &str, name: &str) -> String {
    let head = format!("fn {name}");
    let start = source
        .find(&head)
        .unwrap_or_else(|| panic!("the generated server no longer defines `{head}`"));
    // Back up over the doc comment so the lifted text still reads.
    let rest = &source[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`{head}` has no top-level closing brace"));
    rest[..end + 3].to_string()
}

/// The guard, as a standalone `std`-only program.
fn guard_program() -> String {
    let main_rs = ServerPackage::main_rs();

    let start = main_rs.find(REGION_START).unwrap_or_else(|| {
        panic!("the generated server no longer starts its data-dir resolution with:\n{REGION_START}")
    });
    let end = main_rs[start..]
        .find(REGION_END)
        .map(|e| start + e)
        .unwrap_or_else(|| {
            panic!("the generated server no longer opens the database with:\n{REGION_END}")
        });
    let region = &main_rs[start..end];

    // Non-vacuity: an empty or guard-less region would compile and pass.
    assert!(
        region.contains("refusing to open a database inside the ForgeDB build cache"),
        "the extracted region does not contain the C4 refusal — the anchors have \
         drifted and this test is compiling something else:\n{region}"
    );
    assert!(
        region.contains("std::process::exit(1)"),
        "the extracted region no longer exits on refusal:\n{region}"
    );

    format!(
        "#![allow(unused)]\n\
         fn main() {{\n{region}\n    \
         println!(\"OPENED\");\n}}\n\n{}\n\n{}\n",
        lift_fn(&main_rs, "forgedb_home_dir()"),
        lift_fn(&main_rs, "closest_real_ancestor("),
    )
}

/// Compile the guard once; return the binary's path.
fn compile(dir: &Path) -> PathBuf {
    let src = dir.join("guard.rs");
    std::fs::write(&src, guard_program()).expect("write guard.rs");
    let bin = dir.join("guard");
    let out = Command::new("rustc")
        // The `init` scaffold is edition 2021 and the generated body must compile
        // under the CONSUMER's edition, not this workspace's 2024. Compiling it
        // here as 2021 is the same check `the_body_uses_no_edition_2024_only_syntax`
        // makes structurally, made by the compiler instead.
        .args(["--edition", "2021", "-O", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("run rustc");
    assert!(
        out.status.success(),
        "the extracted C4 guard does not compile:\n{}\n\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        std::fs::read_to_string(&src).unwrap_or_default()
    );
    bin
}

fn run(bin: &Path, cwd: &Path, home: &Path, data: &str) -> (bool, String) {
    let out = Command::new(bin)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .env("FORGEDB_DATA", data)
        .env_remove("FORGEDB_TENANT")
        .output()
        .expect("run the guard");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// **Scenario 36.** A tenant root resolving inside the ForgeDB home is refused,
/// the message names the build cache, and it says what to pass instead.
#[test]
fn scenario_36_the_server_refuses_a_database_inside_the_forgedb_home() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    let project = home.join("projects").join("app");
    std::fs::create_dir_all(&project).unwrap();

    // The exact shape a user reaches by following C7's printed path: stand in the
    // cache, take the RELATIVE default data root.
    let (ok, log) = run(&bin, &project, &home, "data");
    assert!(!ok, "the server opened a database inside the cache:\n{log}");
    assert!(
        log.contains("refusing to open a database inside the ForgeDB build cache"),
        "the refusal does not name the build cache:\n{log}"
    );
    assert!(
        log.contains("FORGEDB_DATA"),
        "the refusal does not tell the user what to pass instead:\n{log}"
    );
    assert!(
        log.contains(&home.display().to_string())
            || log.contains(&std::fs::canonicalize(&home).unwrap().display().to_string()),
        "the refusal does not print the home it compared against:\n{log}"
    );
    assert!(!log.contains("OPENED"), "it refused AND opened:\n{log}");
}

/// The control, and it is not decoration: a guard that refused everything would
/// pass the test above while making the server unusable.
#[test]
fn scenario_36_a_data_root_outside_the_home_is_opened() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    let elsewhere = tmp.path().join("my-project");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let (ok, log) = run(&bin, &elsewhere, &home, "data");
    assert!(ok, "a data root outside the home was refused:\n{log}");
    assert!(log.contains("OPENED"), "the guard did not fall through:\n{log}");
}

/// An **absolute** `FORGEDB_DATA` is the remedy the message names, so it has to
/// work — including when it names a directory that does not exist yet, which is
/// the normal case for a first start.
#[test]
fn scenario_36_the_remedy_the_message_names_actually_works() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    let project = home.join("projects").join("app");
    std::fs::create_dir_all(&project).unwrap();

    let absolute = tmp.path().join("var/lib/forgedb/data");
    let (ok, log) = run(&bin, &project, &home, &absolute.display().to_string());
    assert!(
        ok,
        "the absolute root the refusal recommends was itself refused:\n{log}"
    );
    assert!(log.contains("OPENED"), "{log}");
}

/// **The containment test must resolve the DATA side, not just the home side.**
///
/// `forgedb_home_dir()` canonicalizes `$FORGEDB_HOME` itself, and a process's
/// current directory is already canonical, so a home reached through a symlink
/// proves nothing — that arrangement is caught by canonicalizing either side.
/// The case that actually needs `closest_real_ancestor` is an **absolute
/// `FORGEDB_DATA` that reaches the home through a link**: compared lexically,
/// `<link>/projects/app/data` and `<real home>` share no prefix at all, so the
/// guard falls through and the cache is written to.
///
/// It is the dangerous direction — the failure is *permissive* — and no amount
/// of string-asserting the generated text can see it. Mutation-checked: replacing
/// `closest_real_ancestor(&data_dir)` with a plain `current_dir().join(data_dir)`
/// makes exactly this test fail and leaves the other three green.
///
/// The leaf (`data`) deliberately does not exist, which is the normal case for a
/// first start and the reason the helper canonicalizes the closest *existing*
/// ancestor rather than the path itself.
#[test]
#[cfg(unix)]
fn scenario_36_an_absolute_data_root_reaching_the_home_through_a_link_is_refused() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let real_home = tmp.path().join("real-home");
    std::fs::create_dir_all(real_home.join("projects").join("app")).unwrap();
    let link = tmp.path().join("linked-home");
    std::os::unix::fs::symlink(&real_home, &link).unwrap();

    let through_the_link = link.join("projects").join("app").join("data");
    let outside = tmp.path().join("somewhere-else");
    std::fs::create_dir_all(&outside).unwrap();

    let (ok, log) = run(
        &bin,
        &outside,
        &real_home,
        &through_the_link.display().to_string(),
    );
    assert!(
        !ok,
        "an absolute data root reaching the ForgeDB home through a symlink was \
         allowed — the containment check is lexical, and the cache is writable \
         through the link:\n{log}"
    );
    assert!(
        log.contains("refusing to open a database inside the ForgeDB build cache"),
        "{log}"
    );
}
