use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::commands::build::driver;
use forgedb::naming;

const SCHEMA: &str = r#"
User {
  id: +uuid
  email: &string
  name: string
  age: u32
  created_at: +timestamp
}

Post {
  id: +uuid
  title: ^string
  body: string
  author: *User
  created_at: +timestamp
}
"#;

const COLLIDING_SCHEMA: &str = r#"
Post {
  id: +uuid
  headline: string
  views: u32
}
"#;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    fn generate(config: &str, schemas: &[(&str, &str)]) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&proj).unwrap();
        write(&proj.join("forgedb.toml"), config);
        for (rel, body) in schemas {
            write(&proj.join(rel), body);
        }

        for (rel, _) in schemas {
            let out = Command::new(env!("CARGO_BIN_EXE_forgedb"))
                .args(["generate", "all", "--schema", rel])
                .current_dir(&proj)
                .env("FORGEDB_HOME", &home)
                .output()
                .expect("run forgedb generate");
            assert!(
                out.status.success(),
                "`forgedb generate all --schema {rel}` failed:\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }

        Fixture { _tmp: tmp, home }
    }

    fn project_root(&self) -> PathBuf {
        let projects = self.home.join("projects");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&projects)
            .unwrap_or_else(|e| panic!("no cache at {}: {e}", projects.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(
            dirs.len(),
            1,
            "expected exactly one project in the cache, found {dirs:?}"
        );
        dirs.pop().unwrap()
    }

    fn containers(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(self.project_root().join("apps"))
            .expect("apps/ exists")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs
    }

    fn container(&self) -> PathBuf {
        let mut c = self.containers();
        assert_eq!(c.len(), 1, "expected exactly one app container, got {c:?}");
        c.pop().unwrap()
    }

    fn patch_substrate(&self) {
        let manifest = self.project_root().join("Cargo.toml");
        let mut body = read(&manifest);
        body.push_str("\n[patch.crates-io]\n");
        for dir in [
            "storage",
            "storage-native",
            "storage-web",
            "types",
            "changefeed",
            "wal",
            "compaction",
            "txn",
            "coordinator",
            "auth",
            "query-params",
        ] {
            let path = repo_root().join("crates").join(dir);
            assert!(path.is_dir(), "no such substrate crate: {}", path.display());
            body.push_str(&format!(
                "forgedb-{dir} = {{ path = {:?} }}\n",
                path.to_string_lossy()
            ));
        }
        std::fs::write(&manifest, body).unwrap();
    }

    fn cargo(&self, args: &[&str]) -> std::process::Output {
        let compiles = args.first().is_some_and(|a| *a == "build" || *a == "check");
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.args(args);
        if compiles {
            cmd.arg("--target-dir").arg(self.target_dir());
        }
        cmd.current_dir(self.project_root())
            .env_remove("CARGO_TARGET_DIR")
            .output()
            .expect("cargo runs")
    }

    fn target_dir(&self) -> PathBuf {
        self.home.join("cargo-target")
    }
}

fn exported_c_symbols(code: &str) -> BTreeSet<String> {
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let marker = "extern\"C\"fn";
    let mut out = BTreeSet::new();
    let mut attrs = 0usize;
    for chunk in flat.split("no_mangle").skip(1) {
        attrs += 1;
        let pos = chunk
            .find(marker)
            .expect("a no_mangle attribute with no `extern \"C\" fn` after it");
        let name: String = chunk[pos + marker.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(!name.is_empty(), "a no_mangle definition with no name");
        out.insert(name);
    }
    assert_eq!(
        out.len(),
        attrs,
        "two no_mangle definitions share one symbol name inside ONE crate"
    );
    out
}

fn staticlib_of(artifacts: &[driver::Artifact], app_dir: &Path) -> PathBuf {
    let app = forgedb::cache::member_app_name(app_dir).unwrap_or_else(|| {
        panic!(
            "no app-name marker in {} — `cache::reserve` writes one",
            app_dir.display()
        )
    });
    let want = naming::package_name(&app, &naming::PackageKind::Ffi);

    let hits: Vec<&driver::Artifact> = artifacts
        .iter()
        .filter(|a| a.package == want && a.kind == driver::TargetKind::Staticlib)
        .collect();
    match hits.as_slice() {
        [one] => one.path.clone(),
        [] => panic!(
            "cargo reported no staticlib for package `{want}` (app {}).\nIt reported:\n{}",
            app_dir.display(),
            inventory(artifacts)
        ),
        many => panic!(
            "{} staticlibs for package `{want}` — the lookup is ambiguous:\n{}",
            many.len(),
            inventory(artifacts)
        ),
    }
}

fn inventory(artifacts: &[driver::Artifact]) -> String {
    artifacts
        .iter()
        .map(|a| {
            format!(
                "  {:<10} {:<28} {}",
                a.kind.as_str(),
                a.package,
                a.path.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn flat_contains(haystack: &str, needle: &str) -> bool {
    let flat: String = haystack.chars().filter(|c| !c.is_whitespace()).collect();
    let want: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
    flat.contains(&want)
}

const FULL_TARGETS: &str = r#"[project]
id = "s335-cache"
isolated = true

[generate]
targets = ["rust", "api", "openapi", "stubs", "ffi", "node-runtime", "python-runtime", "go-runtime", "browser-replica"]

[runtime]
replication = true
max_cascade_depth = 7

[storage]
fsync = "never"
"#;

#[test]
fn scenario_20_every_consumer_of_an_app_links_one_database() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    let app = fx.container();

    let mut databases = Vec::new();
    let mut stack = vec![app.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "database.rs") {
                databases.push(p);
            }
        }
    }
    assert!(
        databases.is_empty(),
        "the cache holds a private copy of the database: {databases:?}"
    );

    let core = read(&app.join("core/src/lib.rs"));
    assert!(
        core.contains("EXPECTED_SCHEMA_VERSION"),
        "core/src/lib.rs is not the generated database"
    );

    for kind in ["ffi", "napi", "pyo3", "wasm"] {
        let lib = read(&app.join(kind).join("src/lib.rs"));
        assert!(
            flat_contains(&lib, "use forgedb_core as database;"),
            "the {kind} wrapper does not link core"
        );
        assert!(
            !flat_contains(&lib, "mod database;"),
            "the {kind} wrapper still declares its own database module"
        );

        let manifest = read(&app.join(kind).join("Cargo.toml"));
        for pin in [
            "forgedb-storage",
            "forgedb-types",
            "forgedb-changefeed",
            "forgedb-wal",
            "forgedb-compaction",
            "forgedb-txn",
            "forgedb-coordinator",
            "forgedb-query-params",
        ] {
            assert!(
                !manifest.contains(pin),
                "the {kind} manifest still pins `{pin}` instead of routing through core:\n{manifest}"
            );
        }
        assert!(
            manifest.contains(r#"path = "../core""#),
            "the {kind} manifest does not depend on core:\n{manifest}"
        );
    }

    assert!(
        flat_contains(&core, "MAX_CASCADE_DEPTH: u32 = 7"),
        "the configured cascade depth did not reach the one database"
    );
}

#[test]
fn scenario_21_the_configured_fsync_policy_reaches_the_emitted_source() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    let core = read(&fx.container().join("core/src/lib.rs"));

    assert!(
        flat_contains(&core, "forgedb_wal::FsyncPolicy::Never"),
        "the configured `fsync = \"never\"` did not reach the emitted source"
    );
    assert!(
        !flat_contains(&core, "forgedb_wal::FsyncPolicy::Always"),
        "the emitted source still forces a durability policy the config disabled"
    );
}

const RUST_ONLY: &str = r#"[project]
id = "s335-rust-only"
isolated = true

[generate]
targets = ["rust"]
"#;

#[test]
fn scenario_22_an_app_with_no_server_carries_no_utoipa() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);
    let app = fx.container();

    assert!(
        !app.join("server").exists(),
        "a `targets = [\"rust\"]` app must emit no server package"
    );

    let manifest = read(&app.join("core/Cargo.toml"));
    assert!(
        !manifest.contains("utoipa"),
        "core pins utoipa with nothing to consume it:\n{manifest}"
    );

    let core = read(&app.join("core/src/lib.rs"));
    assert!(
        !core.contains("ToSchema)"),
        "the ToSchema derive is emitted with no utoipa pinned — this cannot compile"
    );
    assert!(
        !core.contains("utoipa::"),
        "the emission names utoipa with nothing pinning it"
    );
}

#[test]
fn scenario_22_a_server_app_carries_utoipa_in_core_not_in_server() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    let app = fx.container();

    assert!(app.join("server").exists(), "this app should emit a server");
    assert!(
        read(&app.join("core/Cargo.toml")).contains("utoipa"),
        "the derive lives in core, so the pin must too"
    );
    assert!(
        read(&app.join("core/src/lib.rs")).contains("ToSchema)"),
        "a server app's core must carry the derive"
    );
}

const TWO_APPS: &str = r#"[project]
id = "s335-two-apps"
isolated = true

[generate]
targets = ["ffi"]
"#;

#[test]
fn scenario_3_two_apps_in_one_project_emit_disjoint_ffi_symbols() {
    let fx = Fixture::generate(
        TWO_APPS,
        &[
            ("a/schema.forge", SCHEMA),
            ("b/schema.forge", COLLIDING_SCHEMA),
        ],
    );
    let apps = fx.containers();
    assert_eq!(apps.len(), 2, "two schemas must reserve two containers");

    let sym_a = exported_c_symbols(&read(&apps[0].join("ffi/src/lib.rs")));
    let sym_b = exported_c_symbols(&read(&apps[1].join("ffi/src/lib.rs")));

    assert!(
        sym_a.len() > 10 && sym_b.len() > 10,
        "a near-empty symbol set proves nothing"
    );
    let shared: Vec<&String> = sym_a.intersection(&sym_b).collect();
    assert!(
        shared.is_empty(),
        "the two apps export the same C symbols: {shared:?}"
    );

    let name_a = forgedb::cache::member_app_name(&apps[0]).expect("app-name marker");
    let name_b = forgedb::cache::member_app_name(&apps[1]).expect("app-name marker");
    assert_ne!(name_a, name_b, "two apps must get two derived names");

    let strip = |set: &BTreeSet<String>, app: &str| -> BTreeSet<String> {
        set.iter()
            .map(|s| {
                let marker = format!("{app}_");
                let at = s
                    .find(&marker)
                    .unwrap_or_else(|| panic!("`{s}` carries no `{marker}` app prefix"));
                s[at + marker.len()..].to_string()
            })
            .collect()
    };
    let stems_a = strip(&sym_a, &name_a);
    let stems_b = strip(&sym_b, &name_b);
    assert!(
        !stems_a.is_disjoint(&stems_b),
        "the two schemas share a model name, so their symbol STEMS must overlap; \
         if they do not, this test would pass without any prefix at all"
    );
    for stem in [
        "post_get",
        "post_insert",
        "post_update",
        "post_delete",
        "open",
        "close",
    ] {
        assert!(
            stems_a.contains(stem) && stems_b.contains(stem),
            "expected both apps to export the `{stem}` stem: {stems_a:?} / {stems_b:?}"
        );
    }
}

#[test]
#[ignore = "compiles a generated cache workspace; run with --ignored"]
fn scenario_20_the_emitted_cache_workspace_builds() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    fx.patch_substrate();

    let meta = fx.cargo(&["metadata", "--no-deps", "--format-version", "1"]);
    assert!(
        meta.status.success(),
        "`cargo metadata` rejects the cache root:\n{}",
        String::from_utf8_lossy(&meta.stderr)
    );

    let out = fx.cargo(&["build"]);
    assert!(
        out.status.success(),
        "the emitted cache workspace does not build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let libdir = fx.target_dir().join("debug");
    let produced: Vec<String> = std::fs::read_dir(&libdir)
        .expect("target/debug exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    for (kind, want) in [
        ("ffi", vec![".a"]),
        ("ffi", vec![".dylib", ".so", ".dll"]),
        ("napi", vec![".dylib", ".so", ".dll"]),
        ("pyo3", vec![".dylib", ".so", ".dll"]),
    ] {
        assert!(
            produced
                .iter()
                .any(|n| n.contains(kind) && want.iter().any(|e| n.ends_with(e))),
            "no {want:?} artifact for the {kind} package: {produced:?}"
        );
    }
}

#[test]
#[ignore = "compiles two generated cache workspaces; run with --ignored"]
fn scenario_3_the_two_staticlibs_export_disjoint_symbols() {
    if !cfg!(target_os = "macos") && !cfg!(target_os = "linux") {
        eprintln!("skipping: no `nm` assumed on this platform");
        return;
    }

    let fx = Fixture::generate(
        TWO_APPS,
        &[
            ("a/schema.forge", SCHEMA),
            ("b/schema.forge", COLLIDING_SCHEMA),
        ],
    );
    fx.patch_substrate();
    let apps = fx.containers();
    assert_eq!(apps.len(), 2, "two schemas must reserve two containers");

    let out = fx.cargo(&["build", "--message-format=json-render-diagnostics"]);
    assert!(
        out.status.success(),
        "the two-app cache workspace does not build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let artifacts = driver::parse_artifacts(&String::from_utf8_lossy(&out.stdout));

    let libdir = fx.target_dir().join("debug");
    let archives: Vec<PathBuf> = std::fs::read_dir(&libdir)
        .expect("target/debug exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "a"))
        .collect();
    assert_eq!(
        archives.len(),
        2,
        "expected one staticlib per app, found {archives:?}"
    );

    fn defined(archive: &Path) -> BTreeSet<String> {
        let nm = Command::new("nm")
            .arg("-g")
            .arg(archive)
            .output()
            .expect("nm runs");
        let out: BTreeSet<String> = String::from_utf8_lossy(&nm.stdout)
            .lines()
            .filter_map(|l| {
                let mut parts = l.split_whitespace().collect::<Vec<_>>();
                let name = parts.pop()?;
                let kind = parts.pop()?;
                (kind == "T").then(|| name.trim_start_matches('_').to_string())
            })
            .collect();
        assert!(
            !out.is_empty(),
            "nm found no defined symbols in {}:\n{}",
            archive.display(),
            String::from_utf8_lossy(&nm.stderr)
        );
        out
    }

    let mut checked = 0usize;
    for (mine, theirs) in [(0usize, 1usize), (1, 0)] {
        let exports = exported_c_symbols(&read(&apps[mine].join("ffi/src/lib.rs")));
        assert!(exports.len() > 10, "a near-empty export set proves nothing");

        let own = staticlib_of(&artifacts, &apps[mine]);
        let other = staticlib_of(&artifacts, &apps[theirs]);
        assert_ne!(
            own, other,
            "the two apps resolved to the SAME archive — a comparison of a set with itself \
             would report a collision that is not there, or hide one that is"
        );

        let own_syms = defined(&own);
        let other_syms = defined(&other);
        for sym in &exports {
            assert!(
                own_syms.contains(sym),
                "`{sym}` is declared `no_mangle` but is not defined in {}",
                own.display()
            );
            assert!(
                !other_syms.contains(sym),
                "the sibling app's staticlib ALSO defines `{sym}` — a single Go \
                 binary importing both would fail to link"
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} symbols were compared");
}

#[test]
#[ignore = "compiles a generated cache workspace; run with --ignored"]
fn scenario_22_a_serverless_core_builds_without_utoipa() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);
    fx.patch_substrate();

    let out = fx.cargo(&["build"]);
    assert!(
        out.status.success(),
        "a core with no utoipa does not build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[ignore = "compiles a generated cache workspace for wasm32; run with --ignored"]
fn the_replica_member_builds_for_wasm32() {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    let have_wasm = installed
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"))
        .unwrap_or(false);
    if !have_wasm {
        eprintln!("skipping: wasm32-unknown-unknown is not installed");
        return;
    }

    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    fx.patch_substrate();

    let name = format!(
        "{}-wasm",
        fx.container().file_name().unwrap().to_string_lossy()
    );
    let members = read(&fx.project_root().join("Cargo.toml"));
    let pkg = members
        .lines()
        .find(|l| l.contains("/wasm\""))
        .unwrap_or_else(|| panic!("no wasm member in the root manifest:\n{members}"));
    assert!(pkg.contains("wasm"), "sanity: {pkg}");

    let manifest = read(&fx.container().join("wasm/Cargo.toml"));
    let pkg_name = manifest
        .lines()
        .find_map(|l| l.strip_prefix("name = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("the wasm manifest declares a package name");
    assert!(
        pkg_name.ends_with("-wasm"),
        "expected a `-wasm` package, got `{pkg_name}` (derived guess was `{name}`)"
    );

    let out = fx.cargo(&[
        "check",
        "-p",
        &pkg_name,
        "--target",
        "wasm32-unknown-unknown",
    ]);
    assert!(
        out.status.success(),
        "the emitted replica does not build for wasm32:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

impl Fixture {
    fn project_dir(&self) -> PathBuf {
        self._tmp.path().join("proj")
    }

    fn forgedb(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_forgedb"))
            .args(args)
            .current_dir(self.project_dir())
            .env("FORGEDB_HOME", &self.home)
            .output()
            .expect("run forgedb")
    }
}

struct Scratch {
    tmp: tempfile::TempDir,
}

impl Scratch {
    fn new(members: &[&str]) -> Scratch {
        let tmp = tempfile::tempdir().unwrap();
        let list = members
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ");
        write(
            &tmp.path().join("Cargo.toml"),
            &format!("[workspace]\nmembers = [{list}]\nresolver = \"2\"\n"),
        );
        Scratch { tmp }
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn member(&self, name: &str, manifest_tail: &str, src_rel: &str, body: &str) {
        let dir = self.tmp.path().join(name);
        write(
            &dir.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{manifest_tail}"
            ),
        );
        write(&dir.join(src_rel), body);
    }

    fn cargo_home(&self, config: &str) -> PathBuf {
        let home = self.tmp.path().join("cargo-home");
        write(&home.join("config.toml"), config);
        home
    }

    fn target_dir(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }
}

#[test]
fn scenario_26_plan_prints_the_invocations_and_compiles_nothing() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);

    let out = fx.forgedb(&["build", "--plan", "--schema", "schema.forge"]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "`forgedb build --plan` failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let project_root = fx.project_root();
    let manifest = project_root.join("Cargo.toml").display().to_string();
    let core_app = forgedb::cache::member_app_name(&fx.container()).expect("app-name marker");
    let core_selector = format!(
        "-p {}",
        forgedb::naming::package_name(&core_app, &forgedb::naming::PackageKind::Core)
    );
    for needle in [
        "cargo build",
        "--manifest-path",
        manifest.as_str(),
        "--message-format=json-render-diagnostics",
        "profile.release.panic",
        core_selector.as_str(),
    ] {
        assert!(
            stdout.contains(needle),
            "`--plan` never printed `{needle}`:\n{stdout}"
        );
    }

    assert!(
        !project_root.join("target").exists(),
        "`--plan` created a target directory in the cache"
    );
    assert!(
        !fx.project_dir().join("target").exists(),
        "`--plan` created a target directory in the user's project"
    );
}

#[test]
fn scenario_26_plan_and_report_are_refused_together() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);
    let out = fx.forgedb(&[
        "build",
        "--plan",
        "--report",
        "-",
        "--schema",
        "schema.forge",
    ]);
    assert!(!out.status.success(), "the combination must be refused");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("--plan") && text.contains("--report"),
        "{text}"
    );
}

#[test]
fn scenario_28_build_in_a_directory_holding_a_foreign_crate_targets_the_cache() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);

    write(
        &fx.project_dir().join("Cargo.toml"),
        "[package]\nname = \"someone-elses-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &fx.project_dir().join("src/lib.rs"),
        "pub fn unrelated() {}\n",
    );

    let out = fx.forgedb(&["build", "--plan", "--schema", "schema.forge"]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "`forgedb build` failed beside a foreign crate:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cache_manifest = fx.project_root().join("Cargo.toml").display().to_string();
    assert!(
        stdout.contains(&cache_manifest),
        "the build is not anchored on the cache workspace:\n{stdout}"
    );
    assert!(
        !stdout.contains("someone-elses-crate"),
        "the foreign package reached the cargo invocation:\n{stdout}"
    );
    assert!(
        !fx.project_dir().join("target").exists(),
        "something compiled inside the user's project directory"
    );
}

#[test]
fn scenario_25_the_pre_build_guard_refuses_a_real_colliding_workspace() {
    use forgedb::commands::build::driver;

    let scratch = Scratch::new(&["transform-1-2", "engine-1-2"]);
    let bin = "[[bin]]\nname = \"forgedb-transform\"\npath = \"src/main.rs\"\n";
    scratch.member("transform-1-2", bin, "src/main.rs", "fn main() {}\n");
    scratch.member("engine-1-2", bin, "src/main.rs", "fn main() {}\n");

    let err = driver::assert_no_duplicate_artifact_names(scratch.root())
        .expect_err("a real colliding workspace must be refused before any compile")
        .to_string();
    assert!(
        err.contains("transform-1-2") && err.contains("engine-1-2"),
        "the error must name BOTH packages:\n{err}"
    );
    assert!(err.contains("forgedb-transform"), "{err}");

    assert!(
        !scratch.root().join("target").join("release").exists(),
        "the guard compiled something"
    );
}

#[test]
fn scenario_25_a_range_stamped_workspace_passes_the_guard() {
    use forgedb::commands::build::driver;

    let scratch = Scratch::new(&["transform-1-2", "engine-1-2"]);
    scratch.member(
        "transform-1-2",
        "[[bin]]\nname = \"forgedb-transform-1-2\"\npath = \"src/main.rs\"\n",
        "src/main.rs",
        "fn main() {}\n",
    );
    scratch.member(
        "engine-1-2",
        "[[bin]]\nname = \"forgedb-engine-1-2\"\npath = \"src/main.rs\"\n",
        "src/main.rs",
        "fn main() {}\n",
    );

    driver::assert_no_duplicate_artifact_names(scratch.root())
        .expect("range-stamped bins do not collide");
}

const UNWIND_PROBE: &str = r#"fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(|| {
        panic!("crossing the boundary");
    })
    .is_err();
    std::process::exit(if caught { 0 } else { 2 });
}
"#;

const HOSTILE_CARGO_CONFIG: &str = "[profile.release]\npanic = \"abort\"\n";

#[test]
fn scenario_24_the_driver_floor_defeats_a_hostile_cargo_home_config() {
    use forgedb::commands::build::driver::{self, Invocation, Selected, TargetKind};
    use forgedb::naming::PackageKind;

    let scratch = Scratch::new(&["unwind-probe"]);
    scratch.member(
        "unwind-probe",
        "[[bin]]\nname = \"unwind-probe\"\npath = \"src/main.rs\"\n",
        "src/main.rs",
        UNWIND_PROBE,
    );
    let cargo_home = scratch.cargo_home(HOSTILE_CARGO_CONFIG);

    let planned = driver::plan(
        scratch.root(),
        &[Selected {
            package: "unwind-probe".to_string(),
            kind: PackageKind::Ffi,
        }],
        true,
    );
    assert_eq!(planned.len(), 1, "{planned:#?}");

    let env_for = |target: &Path| -> Vec<(String, String)> {
        vec![
            ("CARGO_HOME".to_string(), cargo_home.display().to_string()),
            ("CARGO_TARGET_DIR".to_string(), target.display().to_string()),
        ]
    };

    let guarded_target = scratch.target_dir("t-guarded");
    let guarded = Invocation {
        args: planned[0].args.clone(),
        cwd: planned[0].cwd.clone(),
        env: env_for(&guarded_target),
    };
    let artifacts =
        driver::execute(std::slice::from_ref(&guarded)).expect("the probe workspace builds");
    let bin = artifacts
        .iter()
        .find(|a| a.kind == TargetKind::Bin)
        .unwrap_or_else(|| panic!("no bin in {artifacts:#?}"));
    let status = Command::new(&bin.path).status().expect("run the probe");
    assert_eq!(
        status.code(),
        Some(0),
        "the planted `panic = \"abort\"` reached the build: `catch_unwind` never fired, so \
         the FFI unwind boundary is broken. The driver's `--config` floor did not win."
    );

    let mut args = Vec::new();
    let mut skip = false;
    for arg in &planned[0].args {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--config" {
            skip = true;
            continue;
        }
        args.push(arg.clone());
    }
    assert!(
        args.len() + 2 == planned[0].args.len(),
        "the control must remove exactly one --config pair: {:?}",
        planned[0].args
    );

    let bare_target = scratch.target_dir("t-bare");
    let bare = Invocation {
        args,
        cwd: planned[0].cwd.clone(),
        env: env_for(&bare_target),
    };
    let artifacts = driver::execute(std::slice::from_ref(&bare)).expect("the control builds");
    let bin = artifacts
        .iter()
        .find(|a| a.kind == TargetKind::Bin)
        .unwrap_or_else(|| panic!("no bin in {artifacts:#?}"));
    let status = Command::new(&bin.path)
        .status()
        .expect("run the control probe");
    assert_ne!(
        status.code(),
        Some(0),
        "CONTROL FAILED: without the driver's `--config` floor the planted \
         `panic = \"abort\"` did NOT take effect, so this test proves nothing about the \
         floor. Check that $CARGO_HOME was really redirected."
    );
}

#[test]
fn scenario_27_a_real_cargo_stream_carries_three_distinguishable_kinds() {
    use forgedb::commands::build::driver::{self, TargetKind};

    let scratch = Scratch::new(&["threeway", "plainrlib"]);
    scratch.member(
        "threeway",
        "[lib]\nname = \"threeway\"\ncrate-type = [\"cdylib\", \"rlib\", \"staticlib\"]\npath = \"src/lib.rs\"\n",
        "src/lib.rs",
        "pub fn answer() -> u8 { 7 }\n",
    );
    scratch.member(
        "plainrlib",
        "[lib]\nname = \"plainrlib\"\ncrate-type = [\"rlib\"]\npath = \"src/lib.rs\"\n",
        "src/lib.rs",
        "pub fn answer() -> u8 { 8 }\n",
    );

    let target = scratch.target_dir("t");
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "build",
            "--release",
            "--message-format=json-render-diagnostics",
            "-p",
            "threeway",
            "-p",
            "plainrlib",
        ])
        .current_dir(scratch.root())
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("cargo runs");
    assert!(
        out.status.success(),
        "the three-crate-type probe does not build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let artifacts: Vec<_> = driver::parse_artifacts(&stdout)
        .into_iter()
        .filter(|a| a.package == "threeway")
        .collect();

    for want in [TargetKind::Staticlib, TargetKind::Rlib, TargetKind::Cdylib] {
        let hits: Vec<_> = artifacts.iter().filter(|a| a.kind == want).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one {:?} in {artifacts:#?}\n\ncargo said:\n{stdout}",
            want
        );
        assert!(
            hits[0].path.is_file(),
            "cargo reported {} but nothing is there",
            hits[0].path.display()
        );
    }

    let paths: BTreeSet<_> = artifacts.iter().map(|a| a.path.clone()).collect();
    assert_eq!(paths.len(), artifacts.len(), "{artifacts:#?}");

    assert!(
        stdout.contains(".rmeta"),
        "sanity: cargo should have reported an .rmeta for the plain rlib:\n{stdout}"
    );
    let all: Vec<_> = driver::parse_artifacts(&stdout);
    assert!(
        !all.iter()
            .any(|a| a.path.extension().is_some_and(|e| e == "rmeta")),
        "{all:#?}"
    );
    let plain: Vec<_> = all.iter().filter(|a| a.package == "plainrlib").collect();
    assert_eq!(
        plain.len(),
        1,
        "a plain rlib is ONE artifact even though cargo listed two files: {plain:#?}"
    );
    assert_eq!(plain[0].kind, TargetKind::Rlib);
}

#[test]
fn scenario_25_the_guard_runs_inside_forgedb_build() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);

    for (dir, package) in [
        ("transform-1-2", "planted-transform"),
        ("engine-1-2", "planted-engine"),
    ] {
        let member = fx.container().join(dir);
        write(
            &member.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [[bin]]\nname = \"forgedb-transform\"\npath = \"src/main.rs\"\n"
            ),
        );
        write(&member.join("src/main.rs"), "fn main() {}\n");
    }

    let sync = fx.forgedb(&["generate", "all", "--schema", "schema.forge", "--force"]);
    assert!(
        sync.status.success(),
        "re-generate failed:\n{}\n{}",
        String::from_utf8_lossy(&sync.stdout),
        String::from_utf8_lossy(&sync.stderr)
    );
    let root_manifest = read(&fx.project_root().join("Cargo.toml"));
    assert!(
        root_manifest.contains("transform-1-2") && root_manifest.contains("engine-1-2"),
        "the planted members never reached the workspace root, so `forgedb build` \
         below would pass for the wrong reason:\n{root_manifest}"
    );

    let out = fx.forgedb(&["build", "--schema", "schema.forge"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "`forgedb build` compiled a colliding workspace instead of refusing it:\n{combined}"
    );
    assert!(
        combined.contains("planted-transform") && combined.contains("planted-engine"),
        "the CLI must name BOTH packages:\n{combined}"
    );
    assert!(
        combined.contains("forgedb-transform"),
        "the CLI must name the colliding artifact:\n{combined}"
    );
    assert!(
        !fx.project_root().join("target").join("debug").exists(),
        "the refusal came after a compile, not before it"
    );
}
