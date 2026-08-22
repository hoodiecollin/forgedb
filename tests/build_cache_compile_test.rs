//! **The step-5b cache scenarios (#335 / plan #347), run against real cargo.**
//!
//! Scenarios 3, 20, 21 and 22 live here. Scenario 23 has its own file
//! (`tests/pyo3_component_compile_test.rs`) because it assembles the two cache
//! packages by hand rather than driving the CLI — it has to, since it needs a
//! *component-ref* schema and asserts a **link** result on macOS.
//!
//! # Cargo is never mocked
//!
//! The defect this issue fixes is a misunderstanding of what cargo does; a mock
//! would encode the same misunderstanding and go green. Every test here that
//! makes a claim about compilation runs the real thing:
//!
//! * `cargo metadata` / `cargo build` against the cache workspace ForgeDB wrote,
//! * `nm` over the object code that build produced.
//!
//! The cheap half of each scenario (what is *in* the emitted source) runs by
//! default; the half that compiles is `#[ignore]`d, following the convention in
//! `api_wire_test.rs`, `auto_increment_test.rs` and the ten other files that
//! compile a generated crate.
//!
//! # Why a snapshot could not do any of this
//!
//! Every assertion below is about a file that no `insta` snapshot covers: the
//! contents of the **cache**, not of the output directory. The four wrapper
//! manifests in particular were rewritten in step 5b to pin zero substrate, and
//! the first thing compiling the emitted workspace found was that `ffi/`'s
//! rewritten manifest had dropped `serde_json`, which its own body calls — 140
//! `E0433`s that every string-level test in the tree passed straight through.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::commands::build::driver;
use forgedb::naming;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Two models, a required FK, an index and a unique — enough surface that the
/// FFI/napi/pyo3/wasm wrappers all emit their relation and export ops.
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

/// A second app declaring the SAME model name as the first. Scenario 3's whole
/// point: the exported symbol stems collide, so only the per-app prefix keeps
/// the two symbol sets apart.
const COLLIDING_SCHEMA: &str = r#"
Post {
  id: +uuid
  headline: string
  views: u32
}
"#;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

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

/// One temp project, its own `FORGEDB_HOME`, and the cache it wrote into.
struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    /// Write a project (`forgedb.toml` + one schema per entry) and run
    /// `forgedb generate all` once per schema.
    ///
    /// `FORGEDB_HOME` is redirected into the tempdir. Without it `generate`
    /// claims a project id in the developer's real `~/.forgedb` ledger and
    /// writes a cache outside the fixture — the same hazard `tests/common`
    /// documents.
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

    /// The cache project root — `<home>/projects/<id>`, whatever id was derived.
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

    /// Every app container under this project, sorted by directory name.
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

    /// The one app container. Panics when the fixture has more than one — a
    /// scenario that means to be about two apps says so by calling
    /// [`Fixture::containers`].
    fn container(&self) -> PathBuf {
        let mut c = self.containers();
        assert_eq!(c.len(), 1, "expected exactly one app container, got {c:?}");
        c.pop().unwrap()
    }

    /// Append a `[patch.crates-io]` block pointing every substrate crate at this
    /// checkout, so a build here compiles the source in front of us rather than
    /// whatever happens to be published.
    ///
    /// Proving that the *published* substrate resolves is
    /// `.github/workflows/substrate-reclose.yml`'s job, and it must stay there:
    /// it is the only check that runs outside this repo.
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

    /// Run cargo against the cache workspace.
    ///
    /// `--target-dir` is explicit and `CARGO_TARGET_DIR` is removed: an ambient
    /// env var — **or** a `[build] target-dir` in `$CARGO_HOME/config.toml`,
    /// which is machine-wide and needs no env var at all (#292) — would redirect
    /// this into the directory the outer `cargo test` holds a lock on, and the
    /// test would hang rather than fail.
    fn cargo(&self, args: &[&str]) -> std::process::Output {
        // `metadata` takes no `--target-dir` and errors on one, so the flag is
        // added only for the subcommands that compile.
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

/// Every `#[no_mangle] … extern "C" fn <name>` in an emitted FFI crate.
///
/// Anchored on the **attribute/definition pair**, never on a name pattern: a
/// scanner keyed on `forgedb_` would silently match nothing once the prefix
/// became per-app and every assertion over it would pass vacuously.
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

/// The staticlib cargo built for one app's `ffi` package, given that app's
/// container directory.
///
/// **The container's directory name cannot be used to find it.** That name is
/// [`forgedb::cache::member_hash`], an internal storage key, and #335 §2
/// guarantees it appears in **no** public name — `tests/cache_dir_test.rs`
/// asserts exactly that. Archive names come from the cargo package name, which
/// is [`forgedb::naming::package_name`] over the app's *derived* name. Matching
/// a hash against an archive path therefore never matched anything, which is the
/// bug this scenario carried while `#[ignore]`d (#386).
///
/// Two properties this lookup has and a substring search did not:
///
/// * **cargo says which archive belongs to which package**, via its own
///   `--message-format=json` stream. Nothing here re-derives cargo's
///   package-name-to-lib-name mangling, so the mapping cannot quietly rot the
///   way the hash one did.
/// * **Exactly one hit, or a panic naming what was found.** `find()` returning
///   the first match is how a lookup silently answers with the *sibling's*
///   archive — the one failure that would make this scenario compare a set with
///   itself and report disjointness that proves nothing.
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

/// Every artifact cargo reported, for a panic message that says what was there
/// instead of only what was missing.
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

/// Whitespace-insensitive containment. `prettyplease` renders paths as
/// `forgedb_wal::FsyncPolicy::Never` but wraps long lines, so a literal search
/// over the raw text is a coin flip on formatting.
fn flat_contains(haystack: &str, needle: &str) -> bool {
    let flat: String = haystack.chars().filter(|c| !c.is_whitespace()).collect();
    let want: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
    flat.contains(&want)
}

const FULL_TARGETS: &str = r#"[project]
name = "s335-cache"
isolated = true

[generate]
targets = ["rust", "api", "openapi", "stubs", "ffi", "node-runtime", "python-runtime", "go-runtime", "browser-replica"]

[runtime]
replication = true
max_cascade_depth = 7

[storage]
fsync = "never"
"#;

// ---------------------------------------------------------------------------
// Scenario 20 — every consumer of an app links ONE database.rs
// ---------------------------------------------------------------------------

/// **Scenario 20.** *Given* `[runtime] replication = true, max_cascade_depth = 7`
/// and `[storage] fsync = "never"` · *When* `generate all` emits core, server and
/// all four bindings · *Then* exactly one `database.rs` exists in the cache and
/// every wrapper links it.
///
/// The shipped defect: `generate_all`'s `rust` arm threaded the app's
/// `GenConfig` while the four binding arms called
/// `generate_with_schema_version` — i.e. `GenConfig::DEFAULT`. Under the config
/// above that is **two databases with different durability semantics from one
/// `generate` run**. The config is not decoration: under defaults all five
/// emissions are byte-identical and this test passes with the bug present.
#[test]
fn scenario_20_every_consumer_of_an_app_links_one_database() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    let app = fx.container();

    // There is exactly one generated database in the cache, and it is `core`'s
    // crate root. Asserted over a WALK rather than over the four paths we happen
    // to know about: a fifth consumer added later must not be able to
    // reintroduce a private copy without failing here.
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

    // Every wrapper reaches it through the fixed alias, and pins no substrate of
    // its own — which is what makes their substrate types UNIFY with core's
    // rather than merely resolve to the same version by lockfile coincidence.
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

    // The knob that made the two emissions differ, in the one that survives.
    assert!(
        flat_contains(&core, "MAX_CASCADE_DEPTH: u32 = 7"),
        "the configured cascade depth did not reach the one database"
    );
}

// ---------------------------------------------------------------------------
// Scenario 21 — the configured fsync policy reaches the emitted source
// ---------------------------------------------------------------------------

/// **Scenario 21.** *Given* `fsync = "never"` · *Then* the emitted
/// `core/src/lib.rs` names `forgedb_wal::FsyncPolicy::Never`.
///
/// **Fully qualified, and that is the whole point.** `crates/codegen/src/rust.rs`
/// emits three *unconditional* doc comments containing the bare
/// `` `FsyncPolicy::Always` ``, so a substring search for `Always` — or for
/// `FsyncPolicy::Never`'s absence — passes while the bug is present.
/// [`scenario_21_the_bare_name_assertion_would_be_vacuous`] is the control that
/// keeps that fact true rather than merely remembered.
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

/// The control for scenario 21: prove the trap it names is still armed.
///
/// If this ever fails, the doc comments moved and the fully-qualified assertion
/// above stopped being load-bearing — at which point someone will "simplify" it
/// to the bare name and it will silently stop guarding anything.
#[test]
fn scenario_21_the_bare_name_assertion_would_be_vacuous() {
    let fx = Fixture::generate(FULL_TARGETS, &[("schema.forge", SCHEMA)]);
    let core = read(&fx.container().join("core/src/lib.rs"));

    assert!(
        core.contains("FsyncPolicy::Always"),
        "the bare name no longer appears, so scenario 21's warning is stale"
    );
    assert!(
        !flat_contains(&core, "forgedb_wal::FsyncPolicy::Always"),
        "every bare-name occurrence must be prose, never a path"
    );
}

// ---------------------------------------------------------------------------
// Scenario 22 — an app with no server carries no utoipa
// ---------------------------------------------------------------------------

const RUST_ONLY: &str = r#"[project]
name = "s335-rust-only"
isolated = true

[generate]
targets = ["rust"]
"#;

/// **Scenario 22 (source half).** *Given* an app not emitting `server/` · *Then*
/// `core/Cargo.toml` has no `utoipa` dependency and no `ToSchema` derive is
/// emitted.
///
/// The gating is a precondition of the package split rather than a tidy-up
/// beside it: with the derive in `core` and `#[openapi(components(schemas(…)))]`
/// in `server`, the orphan rule blocks supplying the impl from `server`
/// (`E0277: the trait bound 'oc::Post: ToSchema' is not satisfied`).
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

    // Anchored on the DERIVE-LIST token and on the `utoipa::` path, never on the
    // bare trait name: the emission carries an unconditional doc comment saying
    // a scan view is "never `Deserialize`/`ToSchema`", so `!contains("ToSchema")`
    // fails for a prose reason while the code is correct — the same trap
    // scenario 21 names for `FsyncPolicy::Always`, one file over.
    let core = read(&app.join("core/src/lib.rs"));
    assert!(
        !core.contains("ToSchema)"),
        "the ToSchema derive is emitted with no utoipa pinned — this cannot compile"
    );
    assert!(
        !core.contains("utoipa::"),
        "the emission names utoipa with nothing pinning it"
    );
    // The control: prove the bare-name assertion this test does NOT use would be
    // vacuous, so nobody "simplifies" the two above back into it.
    assert!(
        core.contains("ToSchema"),
        "the bare name no longer appears in prose, so the anchoring note is stale"
    );
}

/// The inverse, which is where the orphan rule bites: an app that DOES emit a
/// server must carry utoipa in `core`, not in `server`.
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

// ---------------------------------------------------------------------------
// Scenario 3 — two apps in one project export disjoint FFI symbols
// ---------------------------------------------------------------------------

const TWO_APPS: &str = r#"[project]
name = "s335-two-apps"
isolated = true

[generate]
targets = ["ffi"]
"#;

/// **Scenario 3 (source half).** *Given* two schemas in one project each
/// declaring `Post` · *When* both emit `ffi/` · *Then* their `no_mangle` symbol
/// sets are disjoint.
///
/// Cargo never sees this. The symbols are schema-derived but the `forgedb_`
/// prefix was a **constant**, so two apps declaring a model of the same name
/// exported byte-identical symbols. Under the cdylib path that was a load-time
/// collision only if one process loaded both; the staticlib delivery makes it a
/// **link-time** collision in a single Go binary that imports two ForgeDB
/// packages — reachable, and silent until late.
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

    // Disjointness alone is satisfiable by a prefix on ONE symbol. Both schemas
    // declare `Post`, so the shared stems must be non-empty — that is what
    // proves the prefix is carried by every export rather than by a subset.
    // The prefix is the app's DERIVED NAME, read from the marker `cache::reserve`
    // wrote — not the member hash, which no longer appears in any public name.
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

// ---------------------------------------------------------------------------
// The halves that run real cargo
// ---------------------------------------------------------------------------

/// **Scenario 20 (compile half).** The cache workspace ForgeDB wrote is a valid
/// cargo workspace and every native package in it builds — against ONE `core`.
///
/// `cargo build`, not `cargo check`: `check` never links, so it cannot see a
/// missing link arg (the pyo3 cdylib on macOS), a duplicate exported symbol, or
/// a staticlib that was never produced. Two of the three defects this issue
/// fixes are invisible to `check`.
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

    // Default members exclude `wasm/` (it imports `forgedb_storage::persist`,
    // which exists only on `wasm32`), so a bare build is the right invocation
    // here — and it is the one a user who follows the printed cache path types.
    let out = fx.cargo(&["build"]);
    assert!(
        out.status.success(),
        "the emitted cache workspace does not build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A build that never reached the linker proves nothing about linking. The
    // ffi staticlib in particular is the artifact Go consumes, and it is the one
    // `cargo check` can never produce.
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

/// **Scenario 3 (link half).** The disjointness holds in OBJECT CODE, not merely
/// in the emitted text.
///
/// The source half above reads `#[no_mangle]` attributes; this one reads the
/// symbol table `nm` finds in the two staticlibs cargo actually produced. That
/// is the artifact a Go binary links, and a link-time collision is what the
/// per-app prefix exists to prevent.
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

    // `json-render-diagnostics` keeps errors human-readable on stderr while
    // putting the artifact inventory on stdout, so a build failure here still
    // reads the way the other scenarios' does.
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

    /// Every defined symbol name in an archive.
    ///
    /// **The exit status is deliberately not asserted.** Xcode's `nm` reports
    /// `Unknown attribute kind` on the rustc-produced bitcode of *some* member
    /// objects and exits 1 while still printing tens of thousands of symbols;
    /// gating on the status makes this test fail for a toolchain-version reason
    /// that has nothing to do with the claim. Emptiness is the real failure.
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
                // `<addr> T _name`, or `T _name` for an undefined-address entry.
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

    // Compared over the C-ABI exports ONLY, taken from each app's own emitted
    // source. Two Rust staticlibs share tens of thousands of *mangled* `std`
    // symbols by construction — that is normal and the linker handles it — so a
    // blanket set-disjointness assertion over `nm` output would fail for a
    // reason that is not this scenario's, and "fixing" it would mean weakening
    // the assertion until it proved nothing.
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

/// **Scenario 22 (compile half).** A `core` with no server, and therefore no
/// utoipa, still builds.
///
/// The gate is easy to get backwards: leaving utoipa on unconditionally also
/// "works", and the failure only appears in the direction the orphan rule
/// blocks. Building the serverless shape is what proves the gating is a
/// *narrowing* rather than a break.
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

/// **The wasm member builds for `wasm32`.** Not a numbered scenario, but the one
/// arm of the storage facade that no other test in this file reaches, and the
/// reason `pub use forgedb_changefeed;` is in `core`'s re-export block: the
/// replica names `forgedb_core::forgedb_changefeed::durable::PersistedEvent`,
/// and without the re-export the emitted replica is an `E0433` (verified by
/// deleting the line and rebuilding).
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

    // The wasm member is deliberately NOT a default member, so it has to be
    // named. `check` rather than `build`: linking a `cdylib` for wasm32 needs no
    // extra tooling, but the thing this guards is resolution of the substrate
    // through `core` on a target where `core`'s own dependency set differs.
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

// ===========================================================================
// Step 6 — the build driver (plan #347 scenarios 24–28)
//
// Appended by the step-6 agent. The *pure* halves of these scenarios live in
// `tests/build_driver_test.rs`; what is here is the half that needs a real
// cargo, because the thing being asserted is what cargo actually does.
//
// Two of these compile and are NOT `#[ignore]`d, departing from this file's
// convention on purpose: the crates they build are hand-written, five lines
// long and dependency-free, so they cost a second rather than the minutes (and
// the network) that building the generated cache costs. Scenario 24 in
// particular is one of the seven the design mandates — an `#[ignore]`d
// mandatory scenario is a scenario that does not run.
// ===========================================================================

impl Fixture {
    /// The user's project directory — where `forgedb.toml` and the schema live,
    /// and the working directory a user would run `forgedb build` from.
    ///
    /// Scenario 28 is *about* that directory, so it has to be reachable.
    fn project_dir(&self) -> PathBuf {
        self._tmp.path().join("proj")
    }

    /// Run the CLI in the project directory with this fixture's `FORGEDB_HOME`.
    fn forgedb(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_forgedb"))
            .args(args)
            .current_dir(self.project_dir())
            .env("FORGEDB_HOME", &self.home)
            .output()
            .expect("run forgedb")
    }
}

/// A hand-written cargo workspace with no dependencies, for the scenarios whose
/// subject is cargo itself rather than anything ForgeDB generated.
///
/// Deliberately not a ForgeDB cache: scenario 24 is about the *driver's*
/// defense against a machine-wide config, and pulling the substrate from
/// crates.io to demonstrate it would make the test about the network.
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

    /// A scratch `CARGO_HOME` holding exactly the config this test plants.
    ///
    /// Redirecting `CARGO_HOME` is what makes the "machine-wide config" hazard
    /// reproducible on a machine that does not have one — and it also keeps the
    /// developer's real config from deciding the result either way.
    fn cargo_home(&self, config: &str) -> PathBuf {
        let home = self.tmp.path().join("cargo-home");
        write(&home.join("config.toml"), config);
        home
    }

    fn target_dir(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }
}

// ---------------------------------------------------------------------------
// Scenario 26 — `--plan` prints the invocations and compiles nothing
// ---------------------------------------------------------------------------

/// **Scenario 26.** *Given* any app · *When* `forgedb build --plan` runs · *Then*
/// the package set and the exact cargo invocations are printed, the profile floor
/// is visible in them, and no `target/` directory appears.
///
/// The exit status is asserted, which is not a formality: **no test asserted
/// `forgedb build` exits 0 before this one**. `tests/project_identity_test.rs`
/// runs the command and reads only its `Project:` line.
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
    // Derived through the same API the CLI uses; the exact format is pinned by
    // golden vectors in `naming_test.rs`, so re-spelling it here would be a
    // second definition that drifts.
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

    // Compiled nothing — in either of the two places a build could have landed.
    assert!(
        !project_root.join("target").exists(),
        "`--plan` created a target directory in the cache"
    );
    assert!(
        !fx.project_dir().join("target").exists(),
        "`--plan` created a target directory in the user's project"
    );
}

/// `--plan` and `--report` are refused together, naming both flags.
///
/// `--plan` compiles nothing, so there are no artifacts; a report written anyway
/// would be an empty document from a command that exited 0 — the reads-as-applied
/// failure this issue deletes everywhere else.
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

// `forgedb build --target wasm` is scenario 34's `build` row, and it lives in
// `tests/removed_surface_test.rs` with the other five. Splitting one rule across
// the three files that implemented it is what lets a row go missing unnoticed.

// ---------------------------------------------------------------------------
// Scenario 28 — the headline defect
// ---------------------------------------------------------------------------

/// **Scenario 28.** *Given* a scratch directory holding a foreign `Cargo.toml`
/// plus a `forgedb.toml` and a schema · *When* `forgedb build` runs · *Then* it
/// targets ForgeDB's own workspace and never the foreign package.
///
/// Reproduced end to end on `develop`: `forgedb build` ran a bare `cargo build`
/// with no `--manifest-path` and no `-p` in the user's working directory, so it
/// compiled the unrelated package, printed `✓ Compiled database (native)` and
/// exited 0.
///
/// `--plan` is what is asserted rather than a full compile, and that is the
/// stronger test rather than the cheaper one: the defect is *which manifest and
/// which packages cargo is given*, which the plan states exactly, while a
/// successful compile would prove only that something built.
#[test]
fn scenario_28_build_in_a_directory_holding_a_foreign_crate_targets_the_cache() {
    let fx = Fixture::generate(RUST_ONLY, &[("schema.forge", SCHEMA)]);

    // An ordinary, unrelated Rust crate, sitting exactly where a user would have
    // one: beside their schema.
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

// ---------------------------------------------------------------------------
// Scenario 25 — the pre-build guard, against real `cargo metadata`
// ---------------------------------------------------------------------------

/// **Scenario 25.** *Given* one workspace whose two packages declare the same
/// `[[bin]]` name · *When* the pre-build guard runs · *Then* it errors naming
/// **both** packages — and it needs no compile to do it.
///
/// The shape is `migrate`'s: a `transform-*` and an `engine-*` package for one
/// app both emitting `forgedb-transform`. Cargo's own response is
/// `warning: output filename collision`, **exit 0**, and one surviving file,
/// while the CLI resolves the transformer by a fixed name — so it can run the
/// wrong hop over a user's data directory at exit 0.
///
/// Run against real `cargo metadata` rather than a JSON literal (the literal
/// case is `build_driver_test.rs`), because the claim being made here is that
/// cargo's *actual* document has the fields the guard reads.
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

    // Nothing was compiled to reach that verdict.
    assert!(
        !scratch.root().join("target").join("release").exists(),
        "the guard compiled something"
    );
}

/// The control: range-stamp the two bins and the same workspace passes.
///
/// Without it the assertion above would still pass if the guard rejected every
/// workspace it was handed.
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

// ---------------------------------------------------------------------------
// Scenario 24 — a hostile $CARGO_HOME/config.toml vs. the FFI unwind boundary
// ---------------------------------------------------------------------------

/// The probe: a bin that catches its own unwind and reports which strategy it
/// was built with, through its exit code.
///
/// * `panic = "unwind"` → `catch_unwind` returns `Err` → exit **0**
/// * `panic = "abort"`  → the process dies on the `panic!` → `Abort trap: 6`,
///   exit 134, and `catch_unwind` never returns at all
///
/// This is the generated `ffi`/`napi` wrappers' `catch_unwind` boundary reduced
/// to its smallest reproducible form. It is exercised through the *real driver
/// plan*, so what is under test is the argument the driver actually emits.
const UNWIND_PROBE: &str = r#"fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(|| {
        panic!("crossing the boundary");
    })
    .is_err();
    std::process::exit(if caught { 0 } else { 2 });
}
"#;

/// The machine-wide setting that breaks it.
const HOSTILE_CARGO_CONFIG: &str = "[profile.release]\npanic = \"abort\"\n";

/// **Scenario 24 ★.** *Given* a planted `$CARGO_HOME/config.toml` setting
/// `profile.release.panic = "abort"` · *When* `forgedb build` runs the ffi
/// package · *Then* a panic crossing the boundary is still caught.
///
/// Cargo's `config.toml` **beats the manifest**, which is why the generated
/// wrappers cannot defend themselves — and `[profile.*]` in a workspace *member*
/// is silently ignored outright, so their `panic = "unwind"` tables read as
/// applied and are not. The defense has to be in the driver, and it has to be a
/// command-line `--config`, which outranks every config file.
///
/// The second half of this test is the mutation control, and it is the point:
/// the same workspace, built with the driver's `--config` arguments stripped,
/// must **fail**. Without it, a test that merely observed exit 0 would pass on a
/// machine where nothing hostile was ever planted.
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

    // The real plan, for a real `ffi` package kind, at the release profile.
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
            // Explicit, because an ambient `CARGO_TARGET_DIR` — or a
            // `[build] target-dir` on the developer's machine (#292) — would
            // redirect this into the directory the outer `cargo test` holds a
            // lock on, and the test would hang rather than fail.
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

    // ---- mutation control -------------------------------------------------
    // Strip exactly the thing under test — the `--config` pairs — and nothing
    // else. The hostile config must then win, which is what proves the
    // assertion above is not passing for some unrelated reason.
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

// ---------------------------------------------------------------------------
// Scenario 27 — three kinds from one package, read out of a REAL cargo stream
// ---------------------------------------------------------------------------

/// **Scenario 27.** *Given* a successful build · *Then* every reported path
/// exists, and the staticlib is distinguishable from the rlib and the cdylib of
/// the same package.
///
/// All three files exist on disk, so existence-checking alone cannot tell them
/// apart — and Go delivery needs the staticlib specifically. `TargetKind` is
/// therefore carried on the artifact rather than re-derived downstream.
///
/// This test is also the **provenance** of `build_driver_test.rs`'s replayed
/// stream: it calls `parse_artifacts` on genuine cargo stdout, so if cargo's
/// JSON shape or its `package_id` spelling ever moves, this fails. The pure test
/// pins the rules; this one pins the format.
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
    // A SECOND member, a plain rlib, exists for one reason: only a lib target
    // whose sole crate type is `rlib` reports an `.rmeta`. `threeway` does not —
    // which this test learned from cargo the hard way, having originally
    // asserted the opposite about the three-crate-type member and failed.
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

    // The three are different files — the whole reason existence-checking cannot
    // do the discriminating.
    let paths: BTreeSet<_> = artifacts.iter().map(|a| a.path.clone()).collect();
    assert_eq!(paths.len(), artifacts.len(), "{artifacts:#?}");

    // `.rmeta` rides along beside a PLAIN rlib in cargo's own `filenames` array
    // and must not be reported: it would make `--print-artifact core` ambiguous.
    //
    // The sanity assertion comes first and is not decoration — if this cargo
    // stopped emitting `.rmeta` altogether, the filter assertion below would go
    // green while testing nothing.
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

// ---------------------------------------------------------------------------
// Scenario 25 (call site) — the guard is not merely correct, it RUNS
// ---------------------------------------------------------------------------

/// **Scenario 25 ★ (call-site half).** *Given* a real cache holding two
/// migrate-owned packages that declare the same `[[bin]]` name · *When*
/// `forgedb build` runs · *Then* it fails naming both, before compiling.
///
/// The two tests above prove `assert_no_duplicate_artifact_names` is *correct*.
/// Neither proves `build::run` ever calls it — and a guard that is fully tested
/// and executes zero times is the exact failure this repo has shipped before
/// (see #345). Mutating the function proves the check works; only driving it
/// through the CLI proves the check runs.
///
/// The collision is planted rather than generated because ForgeDB's own naming
/// cannot produce one any more: `transform-1-2`/`engine-1-2` bins are
/// range-stamped precisely so they do not collide. What is under test is the
/// *guard*, so the condition it guards against has to be manufactured — and it
/// is manufactured in the two `PackageKind` directories `build` does **not**
/// own, which is also the pair the guard's message describes.
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

    // The root manifest is rendered from a disk scan, so one more `generate`
    // is what makes cargo aware of the planted members. It must also NOT prune
    // them: `transform-*`/`engine-*` are migrate's packages, and a `generate`
    // that deleted them would make this test vacuous — hence the assertion.
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
