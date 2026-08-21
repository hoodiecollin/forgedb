//! Guards for the ForgeDB build cache directory (#334, epic #332).
//!
//! Scenario numbers refer to the BDD table in the accepted plan gate (#344).
//! Scenarios 4, 7 and 12 needed `generate` wired to the cache dir (plan step 6,
//! which waited on #333); they are at the bottom of this file and are no longer
//! deferred.
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
// Scenarios 4, 7 and 12 — the wiring (plan step 6)
// ---------------------------------------------------------------------------
//
// These were blocked on #342 and are now real.  They use the CLI as a
// subprocess, because what they assert is a property of an *invocation*: which
// project it resolved, and what it left on disk.

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");
const SCHEMA: &str = "Note {\n  id: +uuid\n  body: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn forgedb(cwd: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .output()
        .expect("forgedb runs")
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The `members = [...]` entries of a workspace root manifest.
fn members_of(project: &Path) -> Vec<String> {
    array_of(project, "members")
}

/// `default-members`, or an empty vec when the key is absent — which is the
/// *correct* rendering whenever the filtered set equals `members` or is empty
/// (#335 §4), not a sign of a missing key.
fn default_members_of(project: &Path) -> Vec<String> {
    array_of(project, "default-members")
}

/// Parse one `key = [ … ]` array of quoted strings out of the workspace root.
///
/// Keyed on the array rather than on "every quoted line in the file", because
/// the root now carries two arrays and conflating them would let a
/// `default-members` entry satisfy an assertion about `members`.
fn array_of(project: &Path, key: &str) -> Vec<String> {
    let src = std::fs::read_to_string(project.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("no workspace root at {}: {e}", project.display()));

    let mut out = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{key} = [")) {
            // The empty-array rendering `members = []` is one line.
            if trimmed.ends_with("[]") {
                return out;
            }
            inside = true;
            continue;
        }
        if inside {
            if trimmed == "]" {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix('"')
                && let Some(name) = rest.split('"').next()
            {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// The app CONTAINERS on disk — `apps/<hash>`, project-relative.
///
/// Distinct from [`members_of`] since #335: a container holds the `schema-path`
/// marker and the per-kind package directories and is **never itself a member**,
/// because a `members` entry naming a directory with no manifest is
/// project-wide fatal. Tests about app liveness and accretion assert on
/// containers; tests about what cargo will build assert on members.
fn containers_of(project: &Path) -> Vec<String> {
    let apps = project.join("apps");
    let Ok(entries) = std::fs::read_dir(&apps) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| format!("apps/{}", e.file_name().to_string_lossy()))
        .collect();
    out.sort();
    out
}

/// **Scenario 4.** Two apps in one project land under ONE workspace root, so
/// they share its `Cargo.lock` and its `target/`.
///
/// Sharing is the entire reason the cache dir is a workspace rather than a
/// directory of unrelated crates — it is what makes the substrate compile once
/// per *project* instead of once per *app*. Asserted through the workspace root's
/// member list plus a real `cargo build`, because "both directories exist" would
/// pass even if each app had its own root.
#[test]
fn scenario_4_two_apps_in_one_project_share_one_workspace() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nname = \"shared\"\n");

    for app in ["api", "web"] {
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
        let out = forgedb(
            &root,
            &env.home,
            &["generate", "rust", "--schema", &format!("apps/{app}/schema.forge")],
        );
        assert!(out.status.success(), "generate {app}:\n{}", combined(&out));
    }

    let project = env.home.join("projects").join("shared");
    let containers = containers_of(&project);
    assert_eq!(containers.len(), 2, "both apps have containers: {containers:?}");

    // The container paths are the hashes of the two PROJECT-RELATIVE schema paths
    // — recomputed here rather than read back, so this also pins that the wiring
    // made the path relative to the project root and not to the CWD.
    for app in ["api", "web"] {
        let expect = format!(
            "apps/{}",
            cache::member_hash(Path::new(&format!("apps/{app}/schema.forge")))
        );
        assert!(containers.contains(&expect), "{expect} in {containers:?}");
    }

    // One lockfile and one target dir, at the root — the property the shared
    // workspace exists for. Stub packages keep this a workspace test rather than
    // a substrate compile.
    //
    // They go INSIDE the containers, one level below them, because a container
    // has no manifest of its own. Until #335 step 5a emits real packages this is
    // what a member looks like.
    for container in &containers {
        let dir = project.join(container).join("core");
        write(
            &dir.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"m{}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
                &container[5..13]
            ),
        );
        write(&dir.join("src/lib.rs"), "");
    }

    // The root was rendered BEFORE those packages existed, so it names none of
    // them — that is the reserve/sync_root ordering, and re-deriving here is what
    // an emission step does after it writes. Without this the `cargo build` below
    // would pass having built nothing, which is the vacuous-green shape this
    // whole issue is about.
    let keep = project.join(&containers[0]);
    cache::sync_root(&project, &keep).expect("sync_root");

    let members = members_of(&project);
    assert_eq!(members.len(), 2, "both apps' packages are members: {members:?}");
    for container in &containers {
        let expect = format!("{container}/core");
        assert!(members.contains(&expect), "{expect} in {members:?}");
    }

    // Every member is a `core`, so the default-member filter is a no-op and the
    // key must be OMITTED rather than written as an equal copy.
    assert!(
        default_members_of(&project).is_empty(),
        "default-members should be omitted when it would equal members"
    );

    // `cargo metadata` is what could never pass before #335: the shipped shape
    // listed containers, which have no manifest, and cargo exits 101 on that.
    let meta = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        meta.status.success(),
        "the cache root is not a loadable workspace:\n{}",
        String::from_utf8_lossy(&meta.stderr)
    );

    let built = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        built.status.success(),
        "cargo build over the cache workspace:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(project.join("Cargo.lock").is_file(), "one lockfile for both apps");
    assert!(project.join("target").is_dir(), "one target dir for both apps");
}

/// **Scenario 7.** Wiping the cache reproduces byte-identical generated
/// *source*.
///
/// This is C1 as **scoped** by the design gate (#343 §4): read literally, "delete
/// the cache and regenerate produces an identical result" contradicts the
/// one-lockfile-per-project mechanism this issue exists to build, and C9's
/// deliberate invalidation. What must hold is that nothing in the cache is an
/// *input* to generation — so the emitted source is identical, while the
/// dependency *resolution* is explicitly free to differ.
#[test]
fn scenario_7_a_wipe_reproduces_identical_generated_source() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nname = \"wipe\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let generate = |out_dir: &str| {
        let out = forgedb(&root, &env.home, &["generate", "rust", "--force", "--output", out_dir]);
        assert!(out.status.success(), "generate:\n{}", combined(&out));
        std::fs::read_to_string(root.join(out_dir).join("database.rs")).unwrap()
    };

    let before = generate("gen-before");
    assert!(env.home.join("projects/wipe").is_dir(), "the cache was populated");

    std::fs::remove_dir_all(&env.home).unwrap();

    let after = generate("gen-after");
    assert_eq!(before, after, "generated source must not depend on the cache");
    assert!(
        env.home.join("projects/wipe").is_dir(),
        "…and the cache rebuilt itself from nothing"
    );
}

/// **Scenario 12.** A 2021 member and a 2024 member under one root build with no
/// resolver warning.
///
/// Scenario 5 pins `resolver = "3"` in the rendered bytes; this is the half that
/// proves what those bytes are *for*. Without the key cargo warns on **every**
/// invocation, in a directory the user never opened — and silently falls back to
/// resolver 1, which unifies features more aggressively than 2/3 and so makes
/// C11's cross-app coupling worse exactly where the design is containing it.
///
/// Mixed editions are not hypothetical here: the `init` scaffold emits
/// `edition = "2021"` while the transform crate emits `edition = "2024"`.
#[test]
fn scenario_12_mixed_edition_members_build_without_a_resolver_warning() {
    let env = scoped_home();
    let project = env.home.join("projects").join("mixed");

    let mut members = Vec::new();
    for (name, edition) in [("old", "2021"), ("new", "2024")] {
        let dir = project.join("apps").join(name);
        write(
            &dir.join("Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"{edition}\"\n"),
        );
        write(&dir.join("src/lib.rs"), "");
        members.push(dir);
    }
    cache::write_workspace_root(&project, &members).unwrap();

    let built = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    let stderr = String::from_utf8_lossy(&built.stderr);

    assert!(built.status.success(), "cargo build failed:\n{stderr}");
    assert!(
        !stderr.contains("resolver"),
        "cargo warned about the resolver in a directory the user never opened:\n{stderr}"
    );
}

/// C7: `generate` names the cache directory it wrote to.
///
/// The constraint exists because the generator identity is *partly self-enforcing
/// today* — a user opens `generated/database.rs` and sees their own models. Move
/// the build into a hashed directory they have never seen and that property is
/// gone unless the path is printed. A guard on the function without a guard on
/// the printing would let it be dropped in a refactor with nothing failing.
#[test]
fn generate_names_the_cache_directory_it_wrote_to() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nname = \"visible\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let out = forgedb(&root, &env.home, &["generate", "rust"]);
    assert!(out.status.success(), "{}", combined(&out));

    let member = cache::member_dir("visible", Path::new("schema.forge")).unwrap();
    assert!(
        combined(&out).contains(&member.display().to_string()),
        "the cache path must be named in the output:\n{}",
        combined(&out)
    );
}

/// C4 has a caller, and it runs on a path nobody configured.
///
/// `assert_not_in_cache` was written with #334's earlier steps and had run **zero
/// times** until the wiring landed: a guard whose wiring is untested is a guard
/// that does not exist. Driven through the CLI rather than the function, because
/// what broke would be the *call*, not the check.
#[test]
fn a_tenant_root_inside_the_cache_is_refused_through_the_cli() {
    let env = scoped_home();
    let inside = env.home.join("projects").join("somewhere");
    std::fs::create_dir_all(&inside).unwrap();

    // No `[tenant] root` anywhere — the hazard is the RELATIVE default resolving
    // against a working directory that happens to be in the cache.
    let out = forgedb(&inside, &env.home, &["tenant", "list"]);

    assert!(!out.status.success(), "must refuse:\n{}", combined(&out));
    assert!(
        combined(&out).contains("build cache"),
        "and say why:\n{}",
        combined(&out)
    );
}

/// A member directory that recorded no schema is **kept**, not collected.
///
/// The conservative direction is deliberate and asymmetric: an unreadable marker
/// means "written by a version that did not record one", and evicting a live
/// member forces a full regenerate-and-recompile rather than an incremental
/// rebuild. Guessing wrong in the other direction costs a stale directory.
#[test]
fn a_member_with_no_recorded_schema_is_kept() {
    let env = scoped_home();
    let project = env.home.join("projects").join("legacy");
    let mystery = project.join("apps").join("deadbeefdeadbeef");
    std::fs::create_dir_all(&mystery).unwrap();
    let mine = project.join("apps").join("0000000000000000");

    let live = cache::live_members(&project, &mine).unwrap();
    assert!(live.contains(&mystery), "kept: {live:?}");
    assert!(live.contains(&mine));
}

/// The member set **accretes** rather than being remembered, and a member whose
/// schema is gone drops out of it.
///
/// Not in #344's table. It is the mechanism scenario 4 depends on — generating
/// one app has no knowledge of its siblings, so the set has to be rebuilt from
/// disk each time — and the failure it guards is silent: a stale member keeps a
/// deleted app's `target/` alive forever and keeps appearing in a manifest that
/// claims to be a pure function of the live set.
#[test]
fn a_member_whose_schema_is_gone_drops_out_of_the_set() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nname = \"accrete\"\n");

    for app in ["keep", "doomed"] {
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
        let out = forgedb(
            &root,
            &env.home,
            &["generate", "rust", "--schema", &format!("apps/{app}/schema.forge")],
        );
        assert!(out.status.success(), "{}", combined(&out));
    }

    let project = env.home.join("projects").join("accrete");
    let containers = containers_of(&project);
    assert_eq!(containers.len(), 2);

    // Give each app a stub package, so the manifest's member list reflects the
    // live set rather than being empty. Until step 5a emits real packages this
    // is what a member looks like — and without one the assertion below would
    // pass vacuously against `members = []`.
    for container in &containers {
        let dir = project.join(container).join("core");
        write(
            &dir.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"m{}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
                &container[5..13]
            ),
        );
        write(&dir.join("src/lib.rs"), "");
    }

    std::fs::remove_dir_all(root.join("apps/doomed")).unwrap();
    let out = forgedb(
        &root,
        &env.home,
        &["-v", "generate", "rust", "--force", "--schema", "apps/keep/schema.forge"],
    );
    assert!(out.status.success(), "{}", combined(&out));

    // The dead app's PACKAGE leaves the manifest, which is the property that
    // matters: a stale member keeps a deleted app's `target/` alive forever and
    // keeps appearing in a file that claims to be a pure function of the live
    // set. Its container directory stays on disk — reaping that is GC's job, and
    // it is reported rather than deleted silently.
    let members = members_of(&project);
    assert_eq!(members.len(), 1, "the deleted app left the set: {members:?}");
    assert!(
        members[0].starts_with(&containers_of(&project)[0])
            || members[0].contains(&cache::member_hash(Path::new("apps/keep/schema.forge"))),
        "the surviving member is not the app we kept: {members:?}"
    );
    assert!(
        combined(&out).contains("Orphaned member"),
        "…and it was reported rather than dropped silently:\n{}",
        combined(&out)
    );
}

// ---------------------------------------------------------------------------
// #335 step 2 — containers, the scan-derived member set, and default-members
// ---------------------------------------------------------------------------

/// Build a project on disk with the given package directories under one
/// container, then derive the root from it.
fn synth_project(env: &EnvGuard, name: &str, kinds: &[&str]) -> (PathBuf, PathBuf) {
    let project = env.home.join("projects").join(name);
    let container = project.join("apps").join("deadbeefdeadbeef");
    std::fs::create_dir_all(&container).unwrap();
    // A container records the schema it belongs to; `live_members` keeps a
    // container whose marker is unreadable, so an absent one is also fine here.
    for kind in kinds {
        let dir = container.join(kind);
        write(
            &dir.join("Cargo.toml"),
            &format!("[package]\nname = \"p-{}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n", kind),
        );
        write(&dir.join("src/lib.rs"), "");
    }
    cache::sync_root(&project, &container).expect("sync_root");
    (project, container)
}

/// **#335 §1.** The container itself is never a member. A `members` entry naming
/// a directory with no manifest is project-wide fatal — it is the shape #334
/// shipped, and the reason nothing has ever built in this cache.
#[test]
fn a_container_is_never_a_member() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "containers", &["core"]);

    let members = members_of(&project);
    assert_eq!(members, vec!["apps/deadbeefdeadbeef/core".to_string()]);
    assert!(
        !members.contains(&"apps/deadbeefdeadbeef".to_string()),
        "the container was listed as a member: {members:?}"
    );

    let meta = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        meta.status.success(),
        "cache root is not loadable:\n{}",
        String::from_utf8_lossy(&meta.stderr)
    );
}

/// **#335 §3.** A container with no packages contributes nothing and breaks
/// nothing — the REST-only app. `members = []` is a legal workspace.
#[test]
fn a_container_with_no_packages_renders_as_nothing() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "restonly", &[]);

    assert!(members_of(&project).is_empty());
    let meta = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        meta.status.success(),
        "an empty member set should still load:\n{}",
        String::from_utf8_lossy(&meta.stderr)
    );
}

/// **#335 §4.** `wasm` is excluded from `default-members` so a bare `cargo build`
/// at the root does not try to build the replica for the host triple. C7 prints
/// this path, so a user will eventually `cd` here and type that.
#[test]
fn default_members_excludes_wasm_and_stays_a_subset() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "withwasm", &["core", "wasm"]);

    let members = members_of(&project);
    let defaults = default_members_of(&project);

    assert_eq!(members.len(), 2, "{members:?}");
    assert_eq!(defaults, vec!["apps/deadbeefdeadbeef/core".to_string()]);

    // Not a subset is PROJECT-WIDE FATAL — it breaks `build`, `build -p <a valid
    // member>` and `metadata` alike.
    for d in &defaults {
        assert!(members.contains(d), "{d} is not in members {members:?}");
    }
}

/// **#335 §4.** When the filter changes nothing, the key is OMITTED rather than
/// written as an equal copy — two lists that must stay identical is the skew
/// this derivation exists to prevent.
#[test]
fn default_members_is_omitted_when_it_would_equal_members() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "allnative", &["core", "server", "ffi"]);

    assert_eq!(members_of(&project).len(), 3);
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("default-members"),
        "default-members should be absent:\n{manifest}"
    );
}

/// **#335 §4.** An empty filtered set omits the key too. Writing
/// `default-members = []` instead produces cargo's misleading "the workspace has
/// no members". Reachable for a REST-only app with a migration lineage, whose
/// only packages are `transform-*`/`engine-*`.
#[test]
fn default_members_is_omitted_rather_than_emptied() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "lineageonly", &["transform-1-2", "engine-2-3"]);

    assert_eq!(members_of(&project).len(), 2);
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("default-members"),
        "an empty filtered set must omit the key, not write []:\n{manifest}"
    );

    let built = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        built.status.success(),
        "omitting the key must leave a buildable workspace:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
}

/// **#335 §3.** A directory ForgeDB did not emit is left unlisted rather than
/// admitted. Listing it would make ForgeDB responsible for a manifest it does
/// not own, and a broken one is project-wide fatal; not listing it is inert.
#[test]
fn an_unrecognised_directory_is_not_admitted_as_a_member() {
    let env = scoped_home();
    let (project, container) = synth_project(&env, "stray", &["core"]);

    // A plausible-looking stray with a perfectly valid manifest.
    let stray = container.join("scratch");
    write(
        &stray.join("Cargo.toml"),
        "[package]\nname = \"stray\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    write(&stray.join("src/lib.rs"), "");

    cache::sync_root(&project, &container).expect("sync_root");

    let members = members_of(&project);
    assert_eq!(
        members,
        vec!["apps/deadbeefdeadbeef/core".to_string()],
        "an unrecognised directory was admitted: {members:?}"
    );
}

/// **#335 §3.** `reserve` must not touch the workspace root.
///
/// It runs BEFORE emission, so a root rendered there would list the packages of
/// the *previous* run — and an app's very first generate would produce a root
/// naming none of its own packages. That failure is invisible on a warm cache;
/// this is the assertion that makes the split load-bearing rather than stylistic.
#[test]
fn reserve_does_not_write_the_workspace_root() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let schema = root.join("schema.forge");
    write(&schema, SCHEMA);

    let reserved = cache::reserve("fresh", &root, &schema).expect("reserve");

    assert!(reserved.container.is_dir(), "the container was not created");
    assert!(
        !reserved.project.join("Cargo.toml").exists(),
        "reserve wrote the workspace root; it must be sync_root's job"
    );
}

