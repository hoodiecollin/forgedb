use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use forgedb::cache;

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

fn core_package_of(project_id: &str, rel: &str, siblings: &[&str]) -> String {
    let sibs: Vec<PathBuf> = siblings.iter().map(PathBuf::from).collect();
    let app = forgedb::naming::app_name(
        project_id,
        Path::new(rel),
        &sibs,
        forgedb::naming::SymbolNaming::Minimal,
    );
    forgedb::naming::package_name(&app, &forgedb::naming::PackageKind::Core)
}

fn package_of(project_id: &str, rel: &str, siblings: &[&str], kind: forgedb::naming::PackageKind) -> String {
    let sibs: Vec<PathBuf> = siblings.iter().map(PathBuf::from).collect();
    let app = forgedb::naming::app_name(
        project_id,
        Path::new(rel),
        &sibs,
        forgedb::naming::SymbolNaming::Minimal,
    );
    forgedb::naming::package_name(&app, &kind)
}

#[test]
fn scenario_1_member_path_is_a_pure_function_of_its_inputs() {
    let _env = scoped_home();
    let schema = Path::new("apps/api/schema.forge");

    let first = cache::member_dir("acme", schema).expect("member dir");
    let second = cache::member_dir("acme", schema).expect("member dir");

    assert_eq!(first, second);
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

    assert_eq!(cache::member_hash(Path::new("./apps/api/schema.forge")), canonical);

    assert_eq!(
        cache::member_hash(Path::new("apps/./api/schema.forge")),
        canonical
    );

    let joined: PathBuf = ["apps", "api", "schema.forge"].iter().collect();
    assert_eq!(cache::member_hash(&joined), canonical);
}

#[test]
fn scenario_3_case_is_deliberately_not_folded() {
    let lower = cache::member_hash(Path::new("apps/api/schema.forge"));
    let upper = cache::member_hash(Path::new("Apps/api/schema.forge"));

    assert_ne!(
        lower, upper,
        "case folding would make the hash wrong on Linux, where these are two files"
    );
}

#[test]
fn scenario_5_workspace_root_is_virtual_and_pins_resolver_3() {
    let env = scoped_home();
    let project = env.home.join("projects").join("acme");
    let member = project.join("apps").join("deadbeefdeadbeef");

    cache::write_workspace_root(&project, &[member]).expect("write root");

    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&manifest).expect("root manifest parses");

    assert!(parsed.get("workspace").is_some(), "no [workspace] table");
    assert!(parsed.get("package").is_none(), "C2 forbids [package]");
    assert!(parsed.get("lib").is_none(), "C2 forbids a lib target");
    assert!(parsed.get("dependencies").is_none(), "C2 forbids a shared crate");

    assert_eq!(
        parsed["workspace"]["resolver"].as_str(),
        Some("3"),
        "a virtual manifest with no resolver key defaults to resolver 1"
    );
}

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

    assert_eq!(forward, reverse);
}

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

    let resolved = resolved.expect("empty override should fall back to the home dir");
    assert_ne!(resolved, Path::new(""));
    assert!(resolved.is_absolute());
}

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

fn members_of(project: &Path) -> Vec<String> {
    array_of(project, "members")
}

fn default_members_of(project: &Path) -> Vec<String> {
    array_of(project, "default-members")
}

fn array_of(project: &Path, key: &str) -> Vec<String> {
    let src = std::fs::read_to_string(project.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("no workspace root at {}: {e}", project.display()));

    let mut out = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{key} = [")) {
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

#[test]
fn scenario_4_two_apps_in_one_project_share_one_workspace() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nid = \"shared\"\n");

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

    for app in ["api", "web"] {
        let expect = format!(
            "apps/{}",
            cache::member_hash(Path::new(&format!("apps/{app}/schema.forge")))
        );
        assert!(containers.contains(&expect), "{expect} in {containers:?}");
    }

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

    let keep = project.join(&containers[0]);
    cache::sync_root(&project, &keep).expect("sync_root");

    let members = members_of(&project);
    assert_eq!(members.len(), 2, "both apps' packages are members: {members:?}");
    for container in &containers {
        let expect = format!("{container}/core");
        assert!(members.contains(&expect), "{expect} in {members:?}");
    }

    assert!(
        default_members_of(&project).is_empty(),
        "default-members should be omitted when it would equal members"
    );

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

#[test]
fn scenario_7_a_wipe_reproduces_identical_generated_source() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nid = \"wipe\"\n");
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

#[test]
fn generate_names_the_cache_directory_it_wrote_to() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nid = \"visible\"\n");
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

#[test]
fn a_tenant_root_inside_the_cache_is_refused_through_the_cli() {
    let env = scoped_home();
    let inside = env.home.join("projects").join("somewhere");
    std::fs::create_dir_all(&inside).unwrap();

    let out = forgedb(&inside, &env.home, &["tenant", "list"]);

    assert!(!out.status.success(), "must refuse:\n{}", combined(&out));
    assert!(
        combined(&out).contains("build cache"),
        "and say why:\n{}",
        combined(&out)
    );
}

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

#[test]
fn a_member_whose_schema_is_gone_drops_out_of_the_set() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("forgedb.toml"), "[project]\nid = \"accrete\"\n");

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

fn synth_project(env: &EnvGuard, name: &str, kinds: &[&str]) -> (PathBuf, PathBuf) {
    let project = env.home.join("projects").join(name);
    let container = project.join("apps").join("deadbeefdeadbeef");
    std::fs::create_dir_all(&container).unwrap();
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

#[test]
fn default_members_excludes_wasm_and_stays_a_subset() {
    let env = scoped_home();
    let (project, _) = synth_project(&env, "withwasm", &["core", "wasm"]);

    let members = members_of(&project);
    let defaults = default_members_of(&project);

    assert_eq!(members.len(), 2, "{members:?}");
    assert_eq!(defaults, vec!["apps/deadbeefdeadbeef/core".to_string()]);

    for d in &defaults {
        assert!(members.contains(d), "{d} is not in members {members:?}");
    }
}

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

#[test]
fn an_unrecognised_directory_is_not_admitted_as_a_member() {
    let env = scoped_home();
    let (project, container) = synth_project(&env, "stray", &["core"]);

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

#[test]
fn reserve_does_not_write_the_workspace_root() {
    let _env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let schema = root.join("schema.forge");
    write(&schema, SCHEMA);

    let reserved = cache::reserve("fresh", &root, &schema, forgedb::naming::SymbolNaming::Minimal).expect("reserve");

    assert!(reserved.container.is_dir(), "the container was not created");
    assert!(
        !reserved.project.join("Cargo.toml").exists(),
        "reserve wrote the workspace root; it must be sync_root's job"
    );
}

#[test]
fn c9_a_cli_version_change_drops_the_lockfile() {
    let env = scoped_home();
    let (project, container) = synth_project(&env, "c9", &["core"]);

    let lock = project.join("Cargo.lock");
    write(&lock, "# resolved by some earlier run\n");

    let again = cache::sync_root(&project, &container).expect("sync_root");
    assert!(!again.lock_dropped);
    assert!(lock.is_file(), "an unchanged CLI version must not drop the lock");

    write(&project.join("cli-version"), "0.0.0-something-else");
    let bumped = cache::sync_root(&project, &container).expect("sync_root");
    assert!(bumped.lock_dropped, "the drop was not reported");
    assert!(!lock.exists(), "Cargo.lock survived a CLI version change");

    let third = cache::sync_root(&project, &container).expect("sync_root");
    assert!(!third.lock_dropped, "the version was not re-recorded");
}

#[test]
fn c9_fires_when_the_package_set_does_not_move() {
    let env = scoped_home();
    let (project, container) = synth_project(&env, "c9-static", &["core"]);

    let before = members_of(&project);
    write(&project.join("Cargo.lock"), "# stale\n");
    write(&project.join("cli-version"), "0.0.0-older");

    let synced = cache::sync_root(&project, &container).expect("sync_root");

    assert_eq!(
        members_of(&project),
        before,
        "this test is vacuous unless the member set is unchanged"
    );
    assert!(
        synced.lock_dropped,
        "C9 must not be gated on the package set changing"
    );
}

#[test]
fn c9_an_absent_marker_is_treated_as_a_mismatch() {
    let env = scoped_home();
    let (project, container) = synth_project(&env, "c9-absent", &["core"]);

    std::fs::remove_file(project.join("cli-version")).expect("remove marker");
    write(&project.join("Cargo.lock"), "# written by a version that recorded nothing\n");

    let synced = cache::sync_root(&project, &container).expect("sync_root");
    assert!(synced.lock_dropped);
    assert!(!project.join("Cargo.lock").exists());
    assert!(project.join("cli-version").is_file(), "the marker was not written");
}

fn cargo_package_names(project: &Path) -> Vec<String> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(project)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo runs");
    assert!(
        out.status.success(),
        "cargo metadata failed at {}:\n{}",
        project.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let json = String::from_utf8_lossy(&out.stdout);
    let mut names = Vec::new();
    for chunk in json.split("\"name\":\"").skip(1) {
        if let Some(name) = chunk.split('"').next() {
            if name.contains("-core") || name.contains("-server") {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn s335_5_a_cold_cache_first_generate_is_addressable() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"cold\"\n\n[generate]\ntargets = [\"rust\", \"api\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = forgedb(&root, &env.home, &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    let project = env.home.join("projects").join("cold");
    let names = cargo_package_names(&project);

    assert!(
        names.iter().any(|n| n == &core_package_of("cold", "schema.forge", &["schema.forge"])),
        "the app's core is not addressable after its first generate: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == &package_of("cold", "schema.forge", &["schema.forge"], forgedb::naming::PackageKind::Server)),
        "the app's server is not addressable: {names:?}"
    );
}

#[test]
fn s335_6_a_rust_only_app_lists_its_sole_core() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"solo\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);

    let out = forgedb(&root, &env.home, &["generate", "all"]);
    assert!(out.status.success(), "{}", combined(&out));

    let project = env.home.join("projects").join("solo");
    let hash = cache::member_hash(Path::new("schema.forge"));

    let members = members_of(&project);
    assert_eq!(
        members,
        vec![format!("apps/{hash}/core")],
        "the sole core must be listed explicitly: {members:?}"
    );
    assert!(
        cargo_package_names(&project).contains(&core_package_of("solo", "schema.forge", &["schema.forge"])),
        "cargo cannot address the sole core"
    );
}

#[test]
fn s335_10_no_server_means_no_utoipa_anywhere() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"noweb\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "all"]).status.success());

    let hash = cache::member_hash(Path::new("schema.forge"));
    let core = env
        .home
        .join("projects/noweb/apps")
        .join(&hash)
        .join("core");

    let manifest = std::fs::read_to_string(core.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("utoipa"), "{manifest}");

    let lib = std::fs::read_to_string(core.join("src/lib.rs")).unwrap();
    assert!(!lib.contains("use utoipa::"), "the utoipa import survived");
    assert!(
        !lib.lines().any(|l| l.contains("#[derive(") && l.contains("ToSchema")),
        "a ToSchema derive survived"
    );

    assert!(lib.contains("pub use forgedb_storage;"), "core lost its re-exports");
    assert!(lib.contains("pub use forgedb_types;"));
}

#[test]
fn s335_10_a_server_app_carries_utoipa() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"web\"\n\n[generate]\ntargets = [\"rust\", \"api\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "all"]).status.success());

    let hash = cache::member_hash(Path::new("schema.forge"));
    let app = env.home.join("projects/web/apps").join(&hash);

    let manifest = std::fs::read_to_string(app.join("core/Cargo.toml")).unwrap();
    assert!(manifest.contains("utoipa"), "the gate is stuck off:\n{manifest}");

    let lib = std::fs::read_to_string(app.join("core/src/lib.rs")).unwrap();
    assert!(lib.contains("use utoipa::ToSchema;"));

    let server_manifest = std::fs::read_to_string(app.join("server/Cargo.toml")).unwrap();
    assert!(server_manifest.contains(&core_package_of("web", "schema.forge", &["schema.forge"])));
    let main_rs = std::fs::read_to_string(app.join("server/src/main.rs")).unwrap();
    assert!(main_rs.contains("use forgedb_core as database;"));
    assert!(!main_rs.contains(&hash), "the app hash leaked into main.rs");
}

fn plant_package(dir: &Path, name: &str) {
    write(
        &dir.join("Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
    );
    write(&dir.join("src/lib.rs"), "");
}

#[test]
fn s335_12_an_interrupted_prune_leaves_every_app_in_the_project_buildable() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"killed\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );

    for app in ["api", "web"] {
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
        let out = forgedb(
            &root,
            &env.home,
            &["generate", "rust", "--schema", &format!("apps/{app}/schema.forge")],
        );
        assert!(out.status.success(), "generate {app}:\n{}", combined(&out));
    }

    let project = env.home.join("projects").join("killed");
    let doomed_container = project
        .join("apps")
        .join(cache::member_hash(Path::new("apps/api/schema.forge")));
    plant_package(&doomed_container.join("napi"), "planted-napi");

    cache::sync_root(&project, &doomed_container).unwrap();
    assert!(
        members_of(&project).iter().any(|m| m.ends_with("/napi")),
        "the fixture starts with the doomed package LISTED: {:?}",
        members_of(&project)
    );

    let doomed = cache::prunable(
        &doomed_container,
        &[forgedb::naming::PackageKind::Core],
        forgedb::naming::PruneOwner::GenerateBuild,
    )
    .unwrap();
    assert_eq!(doomed, vec![doomed_container.join("napi")]);
    cache::sync_root_excluding(&project, &doomed_container, &doomed).unwrap();

    let members = members_of(&project);
    assert!(
        !members.iter().any(|m| m.ends_with("/napi")),
        "the root still names the package that is about to be deleted: {members:?}"
    );
    assert!(
        doomed_container.join("napi/Cargo.toml").is_file(),
        "the directory is still on disk — that is what makes this the interrupted state"
    );
    assert_eq!(members.len(), 2, "both apps' cores are still members: {members:?}");

    let meta = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    assert!(
        meta.status.success(),
        "the cache root stopped loading after the interrupted prune:\n{}",
        String::from_utf8_lossy(&meta.stderr)
    );
    let meta = String::from_utf8_lossy(&meta.stdout);
    for app in ["apps/api/schema.forge", "apps/web/schema.forge"] {
        assert!(
            meta.contains(&core_package_of(
                "killed",
                app,
                &["apps/api/schema.forge", "apps/web/schema.forge"]
            )),
            "{app} is missing"
        );
    }
}

#[test]
fn s335_13_a_prune_judges_only_the_container_it_was_given() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"siblings\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );

    for app in ["api", "web"] {
        write(&root.join(format!("apps/{app}/schema.forge")), SCHEMA);
        let out = forgedb(
            &root,
            &env.home,
            &["generate", "rust", "--schema", &format!("apps/{app}/schema.forge")],
        );
        assert!(out.status.success(), "generate {app}:\n{}", combined(&out));
    }

    let project = env.home.join("projects").join("siblings");
    let container = |app: &str| {
        project
            .join("apps")
            .join(cache::member_hash(Path::new(&format!("apps/{app}/schema.forge"))))
    };
    plant_package(&container("api").join("napi"), "planted-api-napi");
    plant_package(&container("web").join("napi"), "planted-web-napi");

    let out = forgedb(
        &root,
        &env.home,
        &["generate", "rust", "--force", "--schema", "apps/api/schema.forge"],
    );
    assert!(out.status.success(), "{}", combined(&out));

    assert!(
        !container("api").join("napi").exists(),
        "the acted-on app's undeclared package survived — the prune never ran"
    );
    assert!(
        container("web").join("napi/Cargo.toml").is_file(),
        "the SIBLING's package was deleted by an invocation that never named it"
    );
    assert!(
        members_of(&project).iter().any(|m| m.ends_with("/napi")),
        "and it is still a member: {:?}",
        members_of(&project)
    );
}

#[test]
fn s335_14_generate_rust_does_not_prune_a_declared_napi() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let config = root.join("forgedb.toml");
    write(&config, "[project]\nid = \"declared\"\n\n[generate]\ntargets = [\"all\"]\n");
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "rust"]).status.success());

    let project = env.home.join("projects").join("declared");
    let container = project
        .join("apps")
        .join(cache::member_hash(Path::new("schema.forge")));
    let napi = container.join("napi");
    plant_package(&napi, "planted-napi");

    let out = forgedb(&root, &env.home, &["generate", "rust", "--force"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        napi.join("Cargo.toml").is_file(),
        "`generate rust` pruned a package `targets = [\"all\"]` declares:\n{}",
        combined(&out)
    );

    write(
        &config,
        "[project]\nid = \"declared\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    let out = forgedb(&root, &env.home, &["generate", "rust", "--force"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        !napi.exists(),
        "an undeclared package survived — the prune is not wired to anything:\n{}",
        combined(&out)
    );
    assert!(
        !members_of(&project).iter().any(|m| m.ends_with("/napi")),
        "…and the root still names it: {:?}",
        members_of(&project)
    );
    assert!(
        combined(&out).contains("Pruned"),
        "a deletion in a directory the user never opens must be reported:\n{}",
        combined(&out)
    );
}

#[test]
fn s335_15_generate_does_not_prune_what_migrate_owns() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"owned\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "rust"]).status.success());

    let project = env.home.join("projects").join("owned");
    let container = project
        .join("apps")
        .join(cache::member_hash(Path::new("schema.forge")));
    plant_package(&container.join("transform-1-2"), "planted-transform");
    plant_package(&container.join("engine-1-2"), "planted-engine");
    plant_package(&container.join("napi"), "planted-napi");

    let out = forgedb(&root, &env.home, &["generate", "rust", "--force"]);
    assert!(out.status.success(), "{}", combined(&out));

    assert!(
        container.join("transform-1-2/Cargo.toml").is_file(),
        "`generate` deleted the transformer `migrate build` owns:\n{}",
        combined(&out)
    );
    assert!(
        container.join("engine-1-2/Cargo.toml").is_file(),
        "`generate` deleted the engine hop `migrate engine` owns:\n{}",
        combined(&out)
    );
    assert!(
        !container.join("napi").exists(),
        "the control failed: nothing was pruned at all, so surviving proves nothing"
    );
}

#[test]
fn s335_15_an_unrecognised_directory_is_never_reaped() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"strangers\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "rust"]).status.success());

    let project = env.home.join("projects").join("strangers");
    let container = project
        .join("apps")
        .join(cache::member_hash(Path::new("schema.forge")));
    plant_package(&container.join("something-else"), "planted-stranger");

    assert!(forgedb(&root, &env.home, &["generate", "rust", "--force"]).status.success());

    assert!(container.join("something-else/Cargo.toml").is_file());
    assert!(
        !members_of(&project).iter().any(|m| m.ends_with("/something-else")),
        "…and it is not listed either — kept, but inert: {:?}",
        members_of(&project)
    );
}

#[test]
fn s335_14_check_mode_prunes_nothing() {
    let env = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(
        &root.join("forgedb.toml"),
        "[project]\nid = \"checked\"\n\n[generate]\ntargets = [\"rust\"]\n",
    );
    write(&root.join("schema.forge"), SCHEMA);
    assert!(forgedb(&root, &env.home, &["generate", "rust"]).status.success());

    let project = env.home.join("projects").join("checked");
    let container = project
        .join("apps")
        .join(cache::member_hash(Path::new("schema.forge")));
    plant_package(&container.join("napi"), "planted-napi");

    let out = forgedb(&root, &env.home, &["generate", "rust", "--check"]);
    assert!(out.status.success(), "the check itself must pass:\n{}", combined(&out));
    assert!(
        container.join("napi/Cargo.toml").is_file(),
        "`--check` deleted a package while claiming to touch nothing:\n{}",
        combined(&out)
    );

    assert!(
        forgedb(&root, &env.home, &["generate", "rust", "--force"])
            .status
            .success()
    );
    assert!(
        !container.join("napi").exists(),
        "the control failed: nothing prunes here, so surviving proves nothing"
    );
}
