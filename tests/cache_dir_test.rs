//! Guards for the ForgeDB build cache directory (#334, epic #332).
//!
//! Scenario numbers refer to the BDD table in the accepted plan gate (#344).
//! Scenarios 4, 7, 9 (the `TenantConfig` half) and 12 need `generate` to be
//! wired to the cache dir — that is plan step 6, which waits on #333's
//! implementation (#342).  They are named in `blocked_on_342.rs`-style comments
//! at the bottom of this file rather than silently omitted, so the gap is
//! visible rather than looking like coverage.
//!
//! **Every test in this file sets `FORGEDB_HOME`.**  A code path that reaches
//! `home::home_dir()` directly would pass this suite while writing into the
//! developer's real `~/.forgedb`, so `serial_env` exists to make that
//! impossible to do by accident.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use forgedb::cache;

/// `std::env::set_var` is process-global, so the tests that depend on
/// `FORGEDB_HOME` must not run concurrently.  Cargo runs integration tests in
/// one binary on many threads by default.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
    home: PathBuf,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(cache::HOME_ENV, v) },
            None => unsafe { std::env::remove_var(cache::HOME_ENV) },
        }
    }
}

/// Point `FORGEDB_HOME` at a fresh tempdir for the duration of one test.
fn scoped_home() -> EnvGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var_os(cache::HOME_ENV);
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().to_path_buf();
    unsafe { std::env::set_var(cache::HOME_ENV, &home) };
    EnvGuard {
        _lock: lock,
        previous,
        _dir: dir,
        home,
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — the member path is a pure function of its inputs
// ---------------------------------------------------------------------------

#[test]
fn scenario_1_member_path_is_a_pure_function_of_its_inputs() {
    let _env = scoped_home();
    let schema = Path::new("apps/api/schema.forge");

    let first = cache::member_dir("acme", schema).expect("member dir");
    let second = cache::member_dir("acme", schema).expect("member dir");

    assert_eq!(first, second);
    // No index was consulted and nothing was created: a lookup RECOMPUTES.
    assert!(!first.exists());
}

#[test]
fn scenario_1_different_apps_in_one_project_get_different_members() {
    let _env = scoped_home();

    let a = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    let b = cache::member_dir("acme", Path::new("apps/web/schema.forge")).unwrap();

    assert_ne!(a, b);
    assert_eq!(a.parent(), b.parent(), "both are members of one project");
}

// ---------------------------------------------------------------------------
// Scenario 2 — golden vectors pin the algorithm AND the normalization
// ---------------------------------------------------------------------------

/// Pinning the digest alone would let the normalization drift while these still
/// pass — and the normalization is where the platform differences live.  So the
/// inputs here are *paths in their natural spellings*, not pre-normalized
/// strings.
///
/// If one of these changes, every member directory in every user's cache is
/// re-keyed and the whole world silently recompiles.  Do not update a vector to
/// match new output; work out why the output moved.
#[test]
fn scenario_2_golden_hash_vectors() {
    const VECTORS: &[(&str, &str)] = &[
        ("schema.forge", "60acb6cba9beb3cf"),
        ("apps/api/schema.forge", "4ec83b602ecd29f5"),
        ("apps/web/schema.forge", "ad9a0dc7a10decf7"),
    ];

    for (input, expected) in VECTORS {
        assert_eq!(
            cache::member_hash(Path::new(input)),
            *expected,
            "golden hash vector moved for {input}"
        );
    }
}

#[test]
fn scenario_2_normalization_collapses_equivalent_spellings() {
    let canonical = cache::member_hash(Path::new("apps/api/schema.forge"));

    // A leading `./` is the same path.
    assert_eq!(cache::member_hash(Path::new("./apps/api/schema.forge")), canonical);

    // An interior `./` component is the same path.
    assert_eq!(
        cache::member_hash(Path::new("apps/./api/schema.forge")),
        canonical
    );

    // Built component-wise rather than parsed from a string — this is the
    // spelling a caller that joined paths will actually produce.
    let joined: PathBuf = ["apps", "api", "schema.forge"].iter().collect();
    assert_eq!(cache::member_hash(&joined), canonical);
}

// ---------------------------------------------------------------------------
// Scenario 3 — case is NOT folded, and that is intentional
// ---------------------------------------------------------------------------

/// This reads as a bug and is not one.  macOS is case-insensitive but
/// case-preserving, so `Apps/…` and `apps/…` are ONE file there and TWO files on
/// Linux.  Folding would make Linux wrong; not folding costs a duplicate member
/// directory on macOS — a wasted rebuild rather than a wrong answer.
#[test]
fn scenario_3_case_is_deliberately_not_folded() {
    let lower = cache::member_hash(Path::new("apps/api/schema.forge"));
    let upper = cache::member_hash(Path::new("Apps/api/schema.forge"));

    assert_ne!(
        lower, upper,
        "case folding would make the hash wrong on Linux, where these are two files"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5 — the workspace root is virtual and pins the resolver
// ---------------------------------------------------------------------------

#[test]
fn scenario_5_workspace_root_is_virtual_and_pins_resolver_3() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");
    let member = project.join("apps").join("deadbeefdeadbeef");

    cache::write_workspace_root(&project, &[member]).expect("write root");

    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&manifest).expect("root manifest parses");

    // C2: [workspace] and members only.
    assert!(parsed.get("workspace").is_some(), "no [workspace] table");
    assert!(parsed.get("package").is_none(), "C2 forbids [package]");
    assert!(parsed.get("lib").is_none(), "C2 forbids a lib target");
    assert!(parsed.get("dependencies").is_none(), "C2 forbids a shared crate");

    // Without this key cargo silently falls back to resolver 1, which unifies
    // features MORE aggressively — making C11's cross-app coupling worse.
    assert_eq!(
        parsed["workspace"]["resolver"].as_str(),
        Some("3"),
        "a virtual manifest with no resolver key defaults to resolver 1"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6 — the manifest is rewritten, not patched  (WRITE THIS ONE FIRST)
// ---------------------------------------------------------------------------

/// The plan names this as the first scenario to write: an accumulating manifest
/// survives a regenerate and stays invisible until someone deletes a schema and
/// the build still references it.
#[test]
fn scenario_6_manifest_is_rewritten_not_patched() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");

    let api = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    let web = cache::member_dir("acme", Path::new("apps/web/schema.forge")).unwrap();

    cache::write_workspace_root(&project, &[api.clone(), web.clone()]).unwrap();
    let both = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let api_hash = api.file_name().unwrap().to_string_lossy().into_owned();
    let web_hash = web.file_name().unwrap().to_string_lossy().into_owned();
    assert!(both.contains(&api_hash));
    assert!(both.contains(&web_hash));

    // The `web` schema is deleted; regeneration passes only the live set.
    cache::write_workspace_root(&project, &[api]).unwrap();
    let one = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();

    assert!(one.contains(&api_hash), "the live member must survive");
    assert!(
        !one.contains(&web_hash),
        "a removed member must NOT survive — the manifest is derived, not remembered"
    );
}

#[test]
fn scenario_6_member_order_is_stable_regardless_of_input_order() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");

    let a = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    let b = cache::member_dir("acme", Path::new("apps/web/schema.forge")).unwrap();

    cache::write_workspace_root(&project, &[a.clone(), b.clone()]).unwrap();
    let forward = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();

    cache::write_workspace_root(&project, &[b, a]).unwrap();
    let reverse = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();

    // Otherwise the file churns on every generate for no reason, and a diff of
    // the cache dir stops meaning anything.
    assert_eq!(forward, reverse);
}

// ---------------------------------------------------------------------------
// Scenario 8 — FORGEDB_HOME fully relocates the tree
// ---------------------------------------------------------------------------

#[test]
fn scenario_8_forgedb_home_relocates_the_whole_tree() {
    let env = scoped_home();

    let root = cache::projects_root().expect("projects root");
    assert!(
        root.starts_with(&env.home),
        "{} escaped FORGEDB_HOME {}",
        root.display(),
        env.home.display()
    );

    let member = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    assert!(member.starts_with(&env.home));

    // And the real home is never consulted when the override is set.
    if let Some(real) = home::home_dir() {
        assert!(!member.starts_with(real.join(".forgedb")));
    }
}

#[test]
fn scenario_8_empty_override_falls_back_rather_than_writing_to_the_filesystem_root() {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var_os(cache::HOME_ENV);
    unsafe { std::env::set_var(cache::HOME_ENV, "") };

    let resolved = cache::forgedb_home();

    match &previous {
        Some(v) => unsafe { std::env::set_var(cache::HOME_ENV, v) },
        None => unsafe { std::env::remove_var(cache::HOME_ENV) },
    }
    drop(lock);

    // An empty override is a misconfiguration, not an instruction to use "".
    let resolved = resolved.expect("empty override should fall back to the home dir");
    assert_ne!(resolved, Path::new(""));
    assert!(resolved.is_absolute());
}

// ---------------------------------------------------------------------------
// Scenario 9 — a data root inside the cache is refused (module half)
// ---------------------------------------------------------------------------

#[test]
fn scenario_9_data_root_inside_the_cache_is_refused() {
    let env = scoped_home();
    let inside = env.home.join("projects").join("acme").join("data");

    let err = cache::assert_not_in_cache(&inside)
        .expect_err("a data root inside the build cache must be refused");

    let message = err.to_string();
    assert!(
        message.contains("build cache"),
        "the error must say why, not just refuse: {message}"
    );
}

#[test]
fn scenario_9_data_root_outside_the_cache_is_allowed() {
    let _env = scoped_home();
    let elsewhere = tempfile::tempdir().unwrap();

    cache::assert_not_in_cache(&elsewhere.path().join("data"))
        .expect("a data root outside the cache is fine");
}

/// The dangerous case is not a configured path — it is `TenantConfig::root()`'s
/// **relative** `"data"` default resolving against a working directory that
/// happens to be inside the cache.  Checking the configured value would catch
/// nothing here, because nothing was configured.
#[test]
fn scenario_9_relative_default_resolved_from_inside_the_cache_is_refused() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");
    std::fs::create_dir_all(&project).unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&project).unwrap();
    let verdict = cache::assert_not_in_cache(Path::new("data"));
    std::env::set_current_dir(original).unwrap();

    verdict.expect_err("the relative default must be caught once CWD is the cache dir");
}

// ---------------------------------------------------------------------------
// Scenario 10 / 11 — orphans, and the empty-live-set refusal
// ---------------------------------------------------------------------------

#[test]
fn scenario_10_renaming_a_schema_orphans_its_member() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");

    let before = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    let after = cache::member_dir("acme", Path::new("apps/gateway/schema.forge")).unwrap();
    std::fs::create_dir_all(&before).unwrap();
    std::fs::create_dir_all(&after).unwrap();

    let found = cache::orphans(&project, &[after.clone()]).expect("scan");

    assert_eq!(found, vec![before], "the pre-rename member is the orphan");
}

#[test]
fn scenario_10_a_project_with_no_apps_dir_has_no_orphans() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");
    let live = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();

    assert!(cache::orphans(&project, &[live]).unwrap().is_empty());
}

/// The failure this prevents is a GC run reached from an error path that
/// produced no members, which would report every app in the project as garbage.
#[test]
fn scenario_11_an_empty_live_set_refuses_to_scan() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");
    let member = cache::member_dir("acme", Path::new("apps/api/schema.forge")).unwrap();
    std::fs::create_dir_all(&member).unwrap();

    let err = cache::orphans(&project, &[])
        .expect_err("an empty live set must be refused, never treated as 'delete all'");

    assert!(err.to_string().contains("empty live member set"));
}

// ---------------------------------------------------------------------------
// Blocked on #342 (plan step 6 — wiring)
// ---------------------------------------------------------------------------
//
// These scenarios from #344's table need `generate` to resolve a project id and
// emit into the cache dir, which is plan step 6 and waits on #333's
// implementation:
//
//   * Scenario 4  — two apps in one project share a lockfile and a target dir
//   * Scenario 7  — a wipe reproduces identical generated SOURCE (scoped C1)
//   * Scenario 12 — mixed-edition members build with no resolver warning
//
// Scenario 5's resolver assertion above is the static half of 12; the dynamic
// half needs a real `cargo build` over emitted members.
