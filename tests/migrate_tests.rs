//! `forgedb migrate` after #335 §9 — the app is **named**, never discovered, and
//! everything it builds is a member of ForgeDB's own cache workspace.
//!
//! Two properties are guarded here, plus both halves of the #292
//! target-directory regression.
//!
//! **Scenario 32 — `--schema` is required and nothing falls back.** Every arm
//! takes exactly one `--schema`; when the transformer cannot be found the
//! command hard-errors *naming the cache path*. The fallback to
//! `migrations/transform` is the tempting move and it is wrong twice: it
//! re-emits a `[package]` with no `[workspace]` under whatever foreign cargo
//! root the working directory sits in (#328 verbatim), and it does so on
//! `migrate engine` — the **mandatory** 0.4.0 upgrade command.
//!
//! **Scenario 33 — `--from`/`--to` resolve the right range member.** One
//! `transform/` per app collided across ranges and `run` got whichever built
//! last. That collision pre-existed at the old shared `migrations/transform`
//! default; moving it into a directory the user never opens would have turned a
//! visible collision into an invisible one. So the member is range-stamped and
//! `run` names the range.
//!
//! **Scenario 34 lives in `tests/removed_surface_test.rs`, not here.** `migrate
//! up`, `migrate build -o`, `migrate engine -o` and `migrate run --bin-dir` are
//! four of its six rows; the other two are `build --target wasm` and `init
//! --rust`/`--api-only`. The property is one cross-cutting rule about the CLI's
//! whole removed surface, and the failure it guards against is exactly *one* of
//! them being forgotten — which six cases in three files cannot see.
//!
//! # What happened to the #292 guards
//!
//! #292 was one property split across two derivations: `migrate build` must
//! *report* the path cargo wrote, and `migrate run` must *locate* it — both
//! under a redirected target directory (`CARGO_TARGET_DIR`, or `[build]
//! target-dir` in any `config.toml` on cargo's discovery chain, including the
//! machine-wide `$CARGO_HOME/config.toml`).
//!
//! The **locate** half still lives in `migrate.rs` and is guarded below, more
//! strictly than before: the bin is seeded *only* in the redirected directory,
//! so a `migrate run` that guessed a path would find nothing.
//!
//! The **build** half moved: `cargo_build_transform_bin` is deleted and the
//! general form is the #335 step-6 build driver, which `migrate` now routes
//! through. Its old test could not be carried over as-is, because it depended on
//! `emit_transform` writing `Cargo.toml` only-if-absent — it seeded a
//! dependency-free stub crate and let the CLI keep it. Nothing in the cache is
//! only-if-absent any more (a manifest carried forward is how a bumped substrate
//! pin fails to reach an existing member), so that trick is gone with it. The
//! replacement is `test_migrate_build_reports_the_path_cargo_actually_wrote`
//! below: real cargo, real substrate, `#[ignore]`d for its cost, asserting on
//! the ARTIFACT rather than the exit status — because the defect's signature is
//! a *successful* exit naming a file that is not there.
//!
//! Beside it, `test_migrate_spawns_no_cargo_of_its_own` is the cheap structural
//! guard that the routing is not quietly re-forked.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const V1: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
const V2: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";
const V3: &str =
    "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n  color: string?\n}\n";

/// The project id every fixture claims. Fixed rather than derived so the cache
/// path is predictable; each test has its own `FORGEDB_HOME`, so two tests
/// claiming the same name never meet.
const PROJECT: &str = "migrate-scenarios";

const CONFIG: &str = "[project]\nid = \"migrate-scenarios\"\n\n[generate]\ntargets = [\"all\"]\n";

/// A `forgedb` invocation scoped to `dir`.
///
/// #333: `FORGEDB_HOME` keeps the project-id claim *and* the whole build cache
/// inside the fixture's tempdir. Without it these tests write into the
/// developer's real `~/.forgedb`.
fn forgedb(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir).env("FORGEDB_HOME", home(dir));
    cmd
}

fn home(dir: &Path) -> PathBuf {
    dir.join(".forgedb-home")
}

/// The cache workspace root for the fixture project.
fn project_root(dir: &Path) -> PathBuf {
    home(dir).join("projects").join(PROJECT)
}

/// The one app container beneath it. Every fixture drives a single
/// `schema.forge`, so `apps/` holds exactly one entry and asserting that is
/// itself worth something: a second entry would mean some command reserved a
/// container for a schema path the operator never named.
fn container(dir: &Path) -> PathBuf {
    let apps = project_root(dir).join("apps");
    let mut found: Vec<PathBuf> = fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no app container under {}: {e}", apps.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one app container under {}, found {found:?}",
        apps.display()
    );
    found.pop().unwrap()
}

/// `<slug>-<member hash>-<kind dir>` — the same scheme `src/naming.rs` renders,
/// re-derived here from the container's own name rather than recomputed, so the
/// test cannot agree with a bug in the hash.
fn package_name(dir: &Path, kind: &str) -> String {
    // Read the app's derived name from the marker `cache::reserve` wrote rather
    // than re-spelling the format here. The format is pinned by golden vectors
    // in `naming_test.rs`; a second spelling would be a second definition.
    let app = forgedb::cache::member_app_name(&container(dir))
        .expect("the container records an app-name marker");
    format!("{app}-{kind}")
}

/// Write a `forgedb.toml` + `schema.forge`, then record a two-hop lineage
/// (v1 -> v2 -> v3) by evolving that one schema in place.
///
/// Evolving one file rather than passing three different `--schema` values
/// matters: `--schema` now selects the *app*, so three paths would reserve
/// three cache containers and `container()` could not name the app's own.
fn record_lineage(dir: &Path) {
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    for (name, body) in [("baseline", V1), ("add_note", V2), ("add_color", V3)] {
        fs::write(dir.join("schema.forge"), body).unwrap();
        let out = forgedb(dir)
            .args([
                "migrate",
                "create",
                name,
                "--schema",
                "schema.forge",
            ])
            .output()
            .expect("run migrate create");
        assert!(
            out.status.success(),
            "recording the {name} hop failed:\n{}",
            combined(&out)
        );
    }
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ---------------------------------------------------------------------------
// Scenario 32 — `--schema` is required, and discovery failure is a hard error
// ---------------------------------------------------------------------------

/// Every `migrate` arm requires `--schema`. Asserted arm by arm rather than on
/// one representative: `status` in particular took **no arguments at all** and
/// its options were a unit struct, so it is the arm most likely to be missed.
#[test]
fn test_scenario_32_every_migrate_arm_requires_schema() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    fs::write(dir.join("schema.forge"), V1).unwrap();

    let arms: [&[&str]; 5] = [
        &["migrate", "create", "some-change"],
        &["migrate", "status"],
        &["migrate", "build", "--from", "1", "--to", "2"],
        &[
            "migrate", "run", "--from", "1", "--to", "2", "--src", "a", "--dest", "b",
        ],
        &["migrate", "engine", "--src", "a", "--dest", "b"],
    ];

    for arm in arms {
        let out = forgedb(dir).args(arm).output().expect("run forgedb");
        let log = combined(&out);
        assert!(
            !out.status.success(),
            "`forgedb {}` ran without naming an app:\n{log}",
            arm.join(" ")
        );
        assert!(
            log.contains("--schema"),
            "`forgedb {}` must refuse by naming --schema:\n{log}",
            arm.join(" ")
        );
    }
}

/// A transformer that was never built is a **hard error naming the cache
/// path** — never a quiet fallback to `migrations/transform`, and never an
/// emission into it.
#[test]
fn test_scenario_32_a_missing_transformer_names_the_cache_and_never_falls_back() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    let out = forgedb(dir)
        .args([
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
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);

    assert!(!out.status.success(), "migrate run invented a bin:\n{log}");
    // The cache path, specifically: an error that says "not found" without
    // saying *where* is what made #292 a loop with no exit.
    assert!(
        log.contains(&container(dir).display().to_string()),
        "the error must name the cache member it looked for:\n{log}"
    );
    assert!(
        log.contains("transform-1-2"),
        "and it must name the RANGE, since that is what selects the member:\n{log}"
    );
    assert!(
        !log.contains("migrations/transform"),
        "no fallback to the path that reproduces #328:\n{log}"
    );
    assert!(
        !dir.join("migrations/transform").exists(),
        "nothing may be emitted into the user's tree:\n{log}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 33 — `--from`/`--to` resolve the right range member
// ---------------------------------------------------------------------------

/// Two transformers built for different ranges; the named one runs.
///
/// The old shape could not express this test at all: one `transform/` per app
/// meant `run` got whichever built last, so "the right one" had no referent.
///
/// The bins are faked rather than compiled — what is under test is which name
/// `migrate run` resolves and executes, and a real substrate build would answer
/// that question no better while taking minutes. Cargo itself is never mocked
/// (see the redirect test below, where cargo's own answer *is* the property).
#[test]
#[cfg(unix)]
fn test_scenario_33_run_resolves_the_named_range_member() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    // `migrate status` reserves the container without building anything, so the
    // fixture can address the cache before a single package exists.
    let out = forgedb(dir)
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .expect("run migrate status");
    assert!(
        out.status.success(),
        "migrate status failed:\n{}",
        combined(&out)
    );

    let container = container(dir);
    let bindir = project_root(dir).join("target/release");
    fs::create_dir_all(&bindir).unwrap();

    let mut markers = Vec::new();
    for range in ["1-2", "2-3"] {
        // The member has to exist: a missing one is a different situation with
        // a different remedy, and `migrate run` says so.
        let member = container.join(format!("transform-{range}"));
        fs::create_dir_all(&member).unwrap();
        fs::write(member.join("Cargo.toml"), "[package]\nname = \"stub\"\n").unwrap();

        let marker = dir.join(format!("ran-{range}"));
        let bin = bindir.join(package_name(dir, &format!("transform-{range}")));
        fs::write(
            &bin,
            format!("#!/bin/sh\necho \"$@\" > {}\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        markers.push(marker);
    }
    let (marker_1_2, marker_2_3) = (markers[0].clone(), markers[1].clone());

    let out = forgedb(dir)
        .args([
            "migrate",
            "run",
            "--schema",
            "schema.forge",
            "--from",
            "2",
            "--to",
            "3",
            "--src",
            "data",
            "--dest",
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(
        marker_2_3.is_file(),
        "the 2->3 transformer was named and did not run:\n{log}"
    );
    assert!(
        !marker_1_2.is_file(),
        "the 1->2 transformer ran instead of the one that was named:\n{log}"
    );

    // The inverse, so the pass above cannot be "it always picks the first one".
    fs::remove_file(&marker_2_3).unwrap();
    let out = forgedb(dir)
        .args([
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
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(marker_1_2.is_file(), "the 1->2 range did not run:\n{log}");
    assert!(
        !marker_2_3.is_file(),
        "the 2->3 transformer ran for the 1->2 range:\n{log}"
    );
}

/// `migrate build` writes a **range-stamped member** into the cache, and
/// nothing into the user's tree.
///
/// Both halves of #328 are asserted on the emitted manifest: no `[workspace]`
/// (it is a member of ForgeDB's root, not its own workspace) and a `[[bin]]`
/// named after the range-stamped package (one app's `transform/` and `engine/`
/// used to declare the same bin, which cargo reports as a *warning* at exit 0).
///
/// **`CARGO_NET_OFFLINE` is set, and that is the assertion's scope showing.**
/// Since #335 step 8 `migrate build` really does compile — it routes through the
/// build driver — and a class-C member pins the substrate from crates.io, so a
/// fixture with its own throwaway `FORGEDB_HOME` (hence its own empty `target/`)
/// would fetch and build the whole substrate closure to prove a property about
/// *emission*, which happens strictly before the compile. Offline makes the
/// resolve fail immediately, leaving exactly the emitted files this test is
/// about. The compile half is
/// `test_migrate_build_reports_the_path_cargo_actually_wrote`, which is real and
/// `#[ignore]`d for its cost.
///
/// Nothing here asserts the exit status, and that is deliberate rather than
/// convenient: a build refused for want of a network is not a claim about the
/// emitter.
#[test]
fn test_migrate_build_emits_a_range_stamped_cache_member() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    let out = forgedb(dir)
        .env("CARGO_NET_OFFLINE", "true")
        .args([
            "migrate",
            "build",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);

    let member = container(dir).join("transform-1-2");
    assert!(
        member.join("Cargo.toml").is_file(),
        "the transformer must be emitted as a cache member:\n{log}"
    );
    assert!(
        member.join("src/main.rs").is_file(),
        "and its sources with it:\n{log}"
    );
    assert!(
        !dir.join("migrations/transform").exists(),
        "and nothing may land in the user's tree:\n{log}"
    );

    let manifest = fs::read_to_string(member.join("Cargo.toml")).unwrap();
    let pkg = package_name(dir, "transform-1-2");
    assert!(
        manifest.contains(&format!("name = \"{pkg}\"")),
        "the member must be named for its app AND its range:\n{manifest}"
    );
    // Anchored on the `[[bin]]` section, not on "the name appears somewhere":
    // it appears in `[package]` too, which would pass while the bin was still
    // the old `forgedb-transform` literal.
    let bin_section = manifest
        .split("[[bin]]")
        .nth(1)
        .unwrap_or_else(|| panic!("no [[bin]] section:\n{manifest}"));
    assert!(
        bin_section.contains(&format!("name = \"{pkg}\"")),
        "the [[bin]] must carry the range too:\n{manifest}"
    );
    assert!(
        !manifest.contains("[workspace]"),
        "a member declaring its own workspace leaves the shared lockfile and target/:\n{manifest}"
    );
    assert!(
        !manifest.contains("[profile"),
        "cargo ignores a profile in a non-root member — shipping one is a setting that \
         reads as applied and is not:\n{manifest}"
    );

    // The root manifest is rewritten AFTER emission, so it names the member
    // that was just written. Rendered before, it would list the previous run's
    // packages and `cargo build -p <this one>` would fail with `did not match
    // any packages`.
    let root = fs::read_to_string(project_root(dir).join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("no cache workspace root:\n{e}\n{log}"));
    assert!(
        root.contains("transform-1-2"),
        "the workspace root must name the member that was just emitted:\n{root}"
    );
}

// ---------------------------------------------------------------------------
// #292 — the surviving half, and the obligation for the other one
// ---------------------------------------------------------------------------

/// #292: `migrate run` must locate the bin **where cargo would have put it**,
/// which is not ours to compute — `CARGO_TARGET_DIR` and `[build] target-dir`
/// in any `config.toml` on cargo's discovery chain (including the machine-wide
/// `$CARGO_HOME/config.toml`) both move it.
///
/// The bin is seeded **only** in the redirected directory and never in the
/// cache root's own `target/`, so a `migrate run` that computed a path by hand
/// finds nothing. Cargo is real here on purpose: the property is "does the CLI
/// ask cargo", and a mock would encode the same assumption the bug was.
#[test]
#[cfg(unix)]
fn test_migrate_run_honours_a_redirected_target_dir() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let redirected = dir.join("elsewhere-target");
    record_lineage(dir);

    let out = forgedb(dir)
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .expect("run migrate status");
    assert!(
        out.status.success(),
        "migrate status failed:\n{}",
        combined(&out)
    );

    let container = container(dir);
    let member = container.join("transform-1-2");
    let pkg = package_name(dir, "transform-1-2");
    fs::create_dir_all(member.join("src")).unwrap();
    fs::write(
        member.join("Cargo.toml"),
        format!("[package]\nname = \"{pkg}\"\nversion = \"0.0.0\"\nedition = \"2021\"\nautobins = false\n\n[[bin]]\nname = \"{pkg}\"\npath = \"src/main.rs\"\n"),
    )
    .unwrap();
    fs::write(member.join("src/main.rs"), "fn main() {}\n").unwrap();
    // A root that names the member, so `cargo metadata` at the cache root has
    // something real to answer about. (`migrate build` writes this itself; the
    // fixture writes it by hand so this test never needs a compile.)
    fs::write(
        project_root(dir).join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"3\"\nmembers = [\"apps/{}/transform-1-2\"]\n",
            container.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();

    let marker = dir.join("ran-redirected");
    let bindir = redirected.join("release");
    fs::create_dir_all(&bindir).unwrap();
    let bin = bindir.join(&pkg);
    fs::write(
        &bin,
        format!("#!/bin/sh\necho \"$@\" > {}\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    // The premise: nothing is at the un-redirected location, so finding the bin
    // can only mean cargo was asked.
    assert!(!project_root(dir).join("target/release").join(&pkg).exists());

    let out = forgedb(dir)
        .env("CARGO_TARGET_DIR", &redirected)
        .args([
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
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);

    assert!(
        !log.contains("not found"),
        "migrate run sends the user back to a build that already happened:\n{log}"
    );
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(
        marker.is_file(),
        "the transformer in the redirected target dir never ran:\n{log}"
    );
}

/// The #292 **build** half, for real: `migrate build` must report the path cargo
/// actually wrote, under a **redirected** target directory.
///
/// `cargo_build_transform_bin` was deleted rather than duplicated: it was
/// already the right driver (`.current_dir`, streamed diagnostics, the real
/// `executable` read out of `--message-format=json-render-diagnostics`, and a
/// refusal to return a path it had not `is_file()`-checked) and only ever knew
/// one package, so the general form is #335 step 6's driver — which `migrate
/// build` and `migrate engine` now both route through.
///
/// **Asserted on the artifact, never on the exit status.** The defect's
/// signature is a *successful* exit naming a file that is not there, so an
/// exit-status assertion is green in exactly the broken case. The reported path
/// is parsed back out of the CLI's own success line and then `is_file()`-checked,
/// and it must live under the redirected directory — nothing may be at the
/// un-redirected guess, because that is the path a hand-joined
/// `<root>/target/release/<bin>` would have produced.
///
/// **Cargo, the substrate and the network are all real, which is why this is
/// `#[ignore]`d.** Every fixture here gets its own `FORGEDB_HOME`, so the cache
/// workspace has its own empty `target/` and a run compiles the class-C
/// substrate closure from crates.io from scratch. Mocking cargo is not an
/// option: the property under test is *whether the CLI asks cargo at all*, and
/// a fake would encode the same assumption the bug was.
///
/// Run it with `cargo test -p forgedb --test migrate_tests -- --ignored`.
#[test]
#[ignore = "compiles a real transformer against the published substrate (network + minutes)"]
fn test_migrate_build_reports_the_path_cargo_actually_wrote() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let redirected = dir.join("elsewhere-target");
    record_lineage(dir);

    let out = forgedb(dir)
        .env("CARGO_TARGET_DIR", &redirected)
        .args([
            "migrate",
            "build",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);

    // The CLI prints `Built transformer: <path>`. Read the path back out of the
    // CLI's own report rather than recomputing it — recomputing is the bug.
    let reported = log
        .lines()
        .find_map(|l| l.split_once("Built transformer:").map(|(_, p)| p.trim()))
        .unwrap_or_else(|| panic!("migrate build never reported a transformer:\n{log}"));
    let reported = PathBuf::from(strip_ansi(reported));

    assert!(
        reported.is_file(),
        "migrate build reported {} and nothing is there — #292 verbatim:\n{log}",
        reported.display()
    );
    assert!(
        reported.starts_with(&redirected),
        "the reported path {} is not under the redirected target dir {} — it was \
         joined by hand rather than read from cargo:\n{log}",
        reported.display(),
        redirected.display()
    );

    let pkg = package_name(dir, "transform-1-2");
    let unredirected = project_root(dir).join("target/release").join(&pkg);
    assert!(
        !unredirected.exists(),
        "a binary appeared at the un-redirected guess {}:\n{log}",
        unredirected.display()
    );
}

/// Strip SGR sequences, so a coloured `ui::success` line still yields a path.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// **One driver, not two.** `src/commands/migrate/mod.rs` spawns no cargo process of
/// its own — every cargo interaction it needs (build, and the target-directory
/// lookup `migrate run` uses) goes through
/// `src/commands/build/driver.rs`.
///
/// Anchored on the **spawn token** rather than on a helper's name, per the
/// standing rule that a structural guard anchored on a binding name is not a
/// guard: a re-forked cargo path would necessarily contain `Command::new("cargo")`
/// whatever the function around it was called.
///
/// This is not the same claim as "the build works" — the test above is that.
/// This one is what keeps a second, subtly different invocation from growing
/// back beside the driver, which is how the two paths disagreed about the target
/// directory in the first place (#292).
#[test]
fn test_migrate_spawns_no_cargo_of_its_own() {
    let src = include_str!("../src/commands/migrate/mod.rs");
    let offenders: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("Command::new(\"cargo\")"))
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "migrate.rs spawns cargo directly again — route it through \
         crate::commands::build::driver instead:\n{}",
        offenders
            .iter()
            .map(|(n, l)| format!("  {n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // And the routing is present, not merely the absence of a fork.
    assert!(
        src.contains("driver::execute(&driver::plan("),
        "migrate.rs no longer builds through driver::plan + driver::execute"
    );
    assert!(
        src.contains("driver::assert_no_duplicate_artifact_names"),
        "migrate.rs no longer runs the pre-build collision guard, so a \
         transform/engine bin-name collision would be a cargo WARNING at exit 0"
    );
    assert!(
        src.contains("driver::target_directory("),
        "migrate run no longer asks the driver where cargo writes"
    );
}

// ---------------------------------------------------------------------------
// #438 — an enum reorder reaches the lineage
// ---------------------------------------------------------------------------

const ENUM_V1: &str = "enum Status { Draft  Published  Archived }\n\
                       Post {\n  id: +uuid\n  title: string\n  status: Status\n}\n";
/// The first two variants swapped, and NOTHING else. Every stored `status` byte
/// stays in range, so no read anywhere fails — it just means something different.
const ENUM_V2: &str = "enum Status { Published  Draft  Archived }\n\
                       Post {\n  id: +uuid\n  title: string\n  status: Status\n}\n";

/// End to end through the real binary: reordering an enum's variants must
/// **record a hop**, and must leave the recorded v1 schema alone.
///
/// Not `#[ignore]`d — it runs the CLI and compiles nothing, the same class as
/// the scenario-32/33 tests above.
///
/// Part (c) is the assertion most likely to be left out, and it is the only one
/// that pins the second half of the defect (#442). The `changes.is_empty()`
/// branch of `migrate create` does not merely return: it rewrites
/// `.schema-snapshot.forge` **and** `migrations/schemas/v{current}.forge` with
/// the new source. So before the fix, the run that failed to notice the reorder
/// also overwrote the record of what v1 actually was — destroying the evidence
/// the transformer needs to repair the data. Detection is what keeps that branch
/// untaken.
#[test]
fn test_an_enum_reorder_records_a_hop_and_leaves_v1_intact() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();

    fs::write(dir.join("schema.forge"), ENUM_V1).unwrap();
    let baseline = forgedb(dir)
        .args([
            "migrate",
            "create",
            "baseline",
            "--schema",
            "schema.forge",
        ])
        .output()
        .expect("run migrate create");
    assert!(
        baseline.status.success(),
        "baseline:\n{}",
        combined(&baseline)
    );

    fs::write(dir.join("schema.forge"), ENUM_V2).unwrap();
    let reorder = forgedb(dir)
        .args([
            "migrate",
            "create",
            "swap_status",
            "--schema",
            "schema.forge",
        ])
        .output()
        .expect("run migrate create");
    assert!(reorder.status.success(), "reorder:\n{}", combined(&reorder));

    let said = strip_ansi(&combined(&reorder));
    assert!(
        !said.contains("No schema changes detected"),
        "the reorder went unseen — this is #438 verbatim:\n{said}"
    );

    // (a) a migration record exists.
    let records: Vec<PathBuf> = fs::read_dir(dir.join("migrations"))
        .expect("migrations/ exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(
        records.len(),
        1,
        "expected exactly one recorded migration, got {records:?}\n{said}"
    );

    // (b) it bumps the serial the generated open guard reads.
    let body = fs::read_to_string(&records[0]).unwrap();
    let record: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(record["from_version"], 1, "from_version:\n{body}");
    assert_eq!(record["to_version"], 2, "to_version:\n{body}");

    // (c) the recorded v1 schema still describes the bytes that are on disk.
    let v1 = fs::read_to_string(dir.join("migrations/schemas/v1.forge"))
        .expect("v1.forge was recorded at baseline");
    assert!(
        v1.contains("Draft  Published"),
        "migrations/schemas/v1.forge was overwritten with the NEW variant order. \
         The lineage now asserts that v1 — the version the existing rows were \
         written under — always had the new ordering, and the transformer would \
         reproduce the corruption faithfully. Got:\n{v1}"
    );
    let v2 = fs::read_to_string(dir.join("migrations/schemas/v2.forge"))
        .expect("v2.forge is recorded for the destination version");
    assert!(v2.contains("Published  Draft"), "v2.forge:\n{v2}");
}

/// `Color` lives inside `struct Badge`, which lives inside `[Badge; 4]` on the
/// model. Reordering `Color` moves nothing in `Badge`'s declaration text and
/// nothing in the model field's type — so this is the case a dependency walk
/// that stops at depth one misses while passing everything else.
///
/// It runs through the REAL `to_simple_schema`, which is the only thing that
/// exercises the transitive walk: the differ's own unit tests hand it a
/// `depends_on` list, so they cannot tell whether the CLI computes one
/// correctly.
const NESTED_V1: &str = "enum Color { Red  Green  Blue }\n\n\
                         struct Badge {\n  rank: u32\n  tint: Color\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";
const NESTED_V2: &str = "enum Color { Green  Red  Blue }\n\n\
                         struct Badge {\n  rank: u32\n  tint: Color\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";
/// Same tree, `Badge`'s own two fields swapped instead — the struct-layout half,
/// which exercises the `structs` projection rather than the `enums` one.
const NESTED_V3: &str = "enum Color { Green  Red  Blue }\n\n\
                         struct Badge {\n  tint: Color\n  rank: u32\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";

#[test]
fn test_a_nested_enum_and_struct_change_reach_the_differ() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();

    let create = |body: &str, desc: &str| {
        fs::write(dir.join("schema.forge"), body).unwrap();
        let out = forgedb(dir)
            .args([
                "migrate",
                "create",
                desc,
                "--schema",
                "schema.forge",
            ])
            .output()
            .expect("run migrate create");
        assert!(out.status.success(), "{desc}:\n{}", combined(&out));
        strip_ansi(&combined(&out))
    };

    create(NESTED_V1, "baseline");

    let reorder_enum = create(NESTED_V2, "swap_color");
    assert!(
        reorder_enum.contains("Enum 'Color'") && reorder_enum.contains("Sticker.badges"),
        "an enum nested inside a struct inside a fixed array must project onto the \
         model field that stores it:\n{reorder_enum}"
    );

    let reorder_struct = create(NESTED_V3, "swap_badge_fields");
    assert!(
        reorder_struct.contains("Struct 'Badge'") && reorder_struct.contains("Sticker.badges"),
        "a struct field reorder must be reported against the storing field:\n{reorder_struct}"
    );
}
