//! #337 — artifact delivery, load-time verification, and per-target destinations.
//!
//! Scenarios 7–22 of the gate 2 plan (#353).
//!
//! **Tier 1** (default `cargo test`) is everything that compiles nothing: the
//! consumer-facing text, the structural properties of the delivery table, and a
//! pure classifier over captured tool output.
//!
//! **Tier 2** (`#[ignore]`, `make test-ignored`, nightly) compiles a release
//! cargo workspace and then *runs* the artifact. Those cases exist because a
//! snapshot pass, a `cargo check` and a `cargo build` each miss a different
//! failure: a `mod fingerprint;` naming a file the writer forgot is
//! snapshot-clean and fails to compile, and a `PyInit_` stem mismatch is
//! compile-clean and fails at import.
//!
//! **A missing runtime is a FAILURE here, not a skip.** `#388` set that
//! precedent for `go`, and the reasoning carries: a guard that skips reports
//! green having evaluated nothing, which is strictly worse than red because
//! nobody investigates it.

mod common;

use common::{linked_libraries, load_commands};
use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = r#"
enum Status { Draft, Published }

Author {
  id: +uuid
  email: &string
  name: ^string
  posts: [Post]
}

Post {
  id: +uuid
  title: ^string
  body: string
  views: u32
  status: ^Status
  author: *Author
}
"#;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn config(name: &str, targets: &str) -> String {
    format!(
        "[project]\nname = \"{name}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n"
    )
}

fn project(tag: &str, targets: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-deliver-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("schema.forge"), SCHEMA);
    write(&dir.join("forgedb.toml"), &config(tag, targets));
    dir
}

/// Run the CLI in `dir` with a `FORGEDB_HOME` inside it. The override is
/// correctness, not hygiene: without it `generate` claims a project id in the
/// developer's real ledger and writes cache packages outside the tempdir.
fn forgedb(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .args(args)
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".home"))
        .output()
        .expect("run forgedb")
}

fn ok(out: &std::process::Output, what: &str) -> String {
    assert!(
        out.status.success(),
        "{what} failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Read a file, PANICKING with the path on a miss.
///
/// Never `unwrap_or_default`: an assertion over an empty string stays live,
/// aimed at nothing, and gets easier to satisfy as it becomes meaningless.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn container(dir: &Path, name: &str) -> PathBuf {
    let apps = dir.join(".home/projects").join(name).join("apps");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no apps dir at {}: {e}", apps.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(found.len(), 1, "expected one app under {}", apps.display());
    found.pop().unwrap()
}

/// The runtime this case needs, or a failure naming it.
fn require_tool(tool: &str, arg: &str) {
    let found = Command::new(tool).arg(arg).output().is_ok();
    assert!(
        found,
        "`{tool}` is not on PATH. This case loads a real artifact, and that is the \
         only check that can see the failure it guards, so a missing runtime is a \
         FAILURE rather than a skip — a guard that skips reports green having \
         evaluated nothing."
    );
}

// ===========================================================================
// Tier 1 — consumer-facing text
// ===========================================================================

/// **Scenario 7.** The `package.json` is the consumer's file and lives with the
/// module it names; the cache package holds none at all.
#[test]
fn scenario_7_the_package_json_moved_out_of_the_cache_and_names_the_entry_module() {
    let dir = project("s7", "\"rust\", \"node-runtime\"");
    ok(&forgedb(&dir, &["generate", "node", "--runtime"]), "generate node --runtime");

    let pkg = dir.join("generated/napi/package.json");
    let doc: serde_json::Value = serde_json::from_str(&read(&pkg)).expect("package.json is JSON");
    assert_eq!(
        doc["main"].as_str(),
        Some("index.js"),
        "`main` must name the entry module. Naming the addon directly makes \
         `require()` resolve it and the load check never run — present, correct, \
         and never executed."
    );
    assert_eq!(doc["types"].as_str(), Some("index.d.ts"));

    assert!(
        !container(&dir, "s7").join("napi/package.json").exists(),
        "the cache package still holds a package.json — a user-editable file in a \
         directory the user never opens"
    );
}

/// **Scenario 7b (deviation 2).** A pre-#337 `package.json` is repointed in
/// place, and every other key survives.
#[test]
fn scenario_7b_a_pre_337_package_json_is_repointed_and_nothing_else_is_touched() {
    let dir = project("s7b", "\"rust\", \"node-runtime\"");
    let pkg = dir.join("generated/napi/package.json");
    write(
        &pkg,
        r#"{
  "name": "my-own-name",
  "version": "9.9.9",
  "main": "forgedb.node",
  "scripts": { "test": "echo mine" }
}
"#,
    );
    ok(&forgedb(&dir, &["generate", "node", "--runtime"]), "generate node --runtime");

    let doc: serde_json::Value = serde_json::from_str(&read(&pkg)).unwrap();
    assert_eq!(doc["main"].as_str(), Some("index.js"), "`main` was not repointed");
    assert_eq!(doc["types"].as_str(), Some("index.d.ts"));
    assert_eq!(doc["name"].as_str(), Some("my-own-name"), "a user key was clobbered");
    assert_eq!(doc["version"].as_str(), Some("9.9.9"), "a user key was clobbered");
    assert_eq!(doc["scripts"]["test"].as_str(), Some("echo mine"));
}

/// **Scenario 8.** Ignoring lives in exactly one place, and that place cannot
/// swallow generated source.
#[test]
fn scenario_8_ignoring_lives_in_exactly_one_place() {
    // `init` scaffolds into a FRESH directory — it refuses to overwrite a
    // project that already exists, and a scaffolded project is what the root
    // `.gitignore` half of this scenario is about.
    let parent = std::env::temp_dir().join(format!("forgedb-deliver-s8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    ok(&forgedb(&parent, &["init", "s8app", "--project-name", "s8-init"]), "init");
    let dir = parent.join("s8app");
    write(&dir.join("schema.forge"), SCHEMA);
    write(&dir.join("forgedb.toml"), &config("s8-init", "\"rust\", \"node-runtime\""));
    ok(&forgedb(&dir, &["generate", "node", "--runtime", "--force"]), "generate");

    let root = read(&dir.join(".gitignore"));
    for stale in ["/generated/**/*.a", "/generated/**/*.lib"] {
        assert!(
            !root.contains(stale),
            "the root .gitignore still carries {stale}:\n{root}"
        );
    }

    let out = read(&dir.join("generated/.gitignore"));
    // Compared as PATTERN LINES, not as substrings: the file explains itself in
    // prose, and a substring check would be satisfiable by the comment.
    let patterns: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    for want in ["*.a", "*.lib", "*.node", "*.so", "*.dylib", "!*.js", "!*.d.ts"] {
        assert!(
            patterns.contains(&want),
            "<output>/.gitignore is missing the pattern `{want}`: {patterns:?}"
        );
    }
    // The #338 constraint: an in-tree ForgeDB-owned cargo package is committed
    // source, so nothing here may name a directory, a Rust file or a manifest.
    for pattern in &patterns {
        assert!(
            !pattern.contains('/'),
            "`{pattern}` names a path — a directory pattern here would swallow \
             #338's in-tree package, which is committed source"
        );
        assert!(
            !pattern.contains(".rs") && !pattern.contains("Cargo"),
            "`{pattern}` would un-commit generated Rust source"
        );
    }
}

/// **Scenario 12.** One spelling of the extension stem.
///
/// CPython resolves `PyInit_<stem>` from the delivered filename, so the
/// `#[pymodule]` name and the delivery table's filename are ONE decision. A
/// second literal is how they come apart, and the failure — `dynamic module does
/// not define module export function` — is invisible to `cargo build`, to
/// `cargo check` and to every snapshot.
#[test]
fn scenario_12_the_extension_stem_has_one_spelling() {
    // Assembled at runtime so this file's own source cannot satisfy the search.
    let stem = format!("_forgedb{}native", "_");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let mut hits: Vec<String> = Vec::new();
    let mut stack = vec![root.join("src"), root.join("crates")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("tests") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            // Comments are stripped: a doc comment naming the stem must not be
            // able to satisfy — or to break — this count.
            for (n, line) in src.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if code.contains(&stem) {
                    hits.push(format!("{}:{}", path.display(), n + 1));
                }
            }
        }
    }

    assert_eq!(
        hits.len(),
        1,
        "the extension stem must appear exactly once in non-test code — at \
         `PyO3Generator::EXTENSION_STEM`, which both the #[pymodule] name and the \
         delivery table read. Found: {hits:?}"
    );
    assert!(
        hits[0].contains("pyo3.rs"),
        "the one definition is not in the PyO3 generator: {}",
        hits[0]
    );
}

/// **Scenario 9.** No version strings and no timestamps in generated code.
///
/// Scanned over the emitted OUTPUT, not over the generators' source, so a
/// comment explaining the rule cannot satisfy it. Manifests are excluded on
/// purpose — they carry pins, which is the point of hashing them.
///
/// The allow-list has exactly one entry, named and justified: `ffi.rs` emits
/// `env!("CARGO_PKG_VERSION")` into generated Rust, where it expands to the
/// generated crate's own hardcoded `0.1.0` and never to the CLI's. It is a
/// version-bearing token that is NOT a CLI-version coupling, and a guard written
/// without knowing it is there would "fix" a non-bug.
#[test]
fn scenario_9_generated_code_carries_no_version_string_and_no_timestamp() {
    use forgedb_codegen::{FfiGenerator, GoGenerator, NapiGenerator, PyO3Generator};
    let schema = forgedb_parser::Parser::new(SCHEMA).unwrap().parse().unwrap();
    const SYM: &str = "app_0011223344556677_";
    const FP: &str = "0123456789abcdef";

    let artifacts: Vec<(&str, String)> = vec![
        ("ffi/src/lib.rs", FfiGenerator::generate(&schema, SYM).unwrap().code),
        ("ffi/forgedb.h", FfiGenerator::generate_header(&schema, SYM, FP).unwrap().code),
        ("napi/src/lib.rs", NapiGenerator::generate(&schema).unwrap().code),
        ("napi/index.js", NapiGenerator::entry_module(FP).code),
        ("napi/index.d.ts", NapiGenerator::type_declarations(&schema).unwrap().code),
        ("pyo3/src/lib.rs", PyO3Generator::generate(&schema).unwrap().code),
        ("pyo3/forgedb.py", PyO3Generator::python_module(&schema, FP).unwrap().code),
        ("pyo3/forgedb.pyi", PyO3Generator::type_stub(&schema).unwrap().code),
        ("go/forgedb.go", GoGenerator::generate(&schema, SYM, FP).unwrap().code),
    ];

    // `x.y.z` and an ISO date. Deliberately crude: this is about a CLASS of
    // token, and a precise matcher would be one more thing to keep in step.
    // Punctuation is trimmed off each end FIRST. Without it `0.4.1.` at the end
    // of a sentence tokenises to four parts and slips through — which is exactly
    // how a version would enter generated code, in a comment.
    fn strip(w: &str) -> &str {
        w.trim_matches(|c| c == '.' || c == '-')
    }
    let is_version = |w: &str| {
        let parts: Vec<&str> = strip(w).split('.').collect();
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    };
    let is_date = |w: &str| {
        let parts: Vec<&str> = strip(w).split('-').collect();
        parts.len() == 3
            && parts[0].len() == 4
            && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    };

    let mut violations = Vec::new();
    for (name, code) in &artifacts {
        for (n, line) in code.lines().enumerate() {
            for word in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-')) {
                if is_version(word) || is_date(word) {
                    violations.push(format!("{name}:{}: `{word}` in `{}`", n + 1, line.trim()));
                }
            }
        }
        // Timestamp CALLS, which produce a value rather than a literal.
        for banned in ["SystemTime::now", "Utc::now", "Local::now", "Instant::now"] {
            assert!(
                !code.contains(banned),
                "{name} emits `{banned}` — a timestamp in generated output \
                 reintroduces the coupling a SOURCE fingerprint exists to avoid"
            );
        }
    }
    assert!(
        violations.is_empty(),
        "generated code carries version-like or date-like literals:\n{}",
        violations.join("\n")
    );

    // The allow-list, asserted as an EXACT set rather than tolerated.
    let carriers: Vec<&str> = artifacts
        .iter()
        .filter(|(_, code)| code.contains("CARGO_PKG_VERSION"))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        carriers,
        vec!["ffi/src/lib.rs"],
        "the CARGO_PKG_VERSION allow-list has exactly one entry — the generated \
         engine's own version accessor, which expands to the GENERATED crate's \
         hardcoded 0.1.0, never to the CLI's"
    );
}

/// **Scenario 10.** The delivery table is total over `PackageKind`.
///
/// A wildcard arm makes a newly added kind a silent non-delivery — the exact
/// failure this issue removes. Rust cannot express "adding a variant must break
/// this" as a test, so the guard is structural, and it is scoped to the
/// FUNCTION BODY with a lookup that PANICS on a miss: a scoping query that
/// degrades to the whole file leaves the assertion live and aimed at the wrong
/// subject, where it only gets easier to satisfy.
#[test]
fn scenario_10_the_delivery_table_has_no_wildcard_arm() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands/build/deliver.rs");
    let src = read(&path);

    let start = src
        .find("pub fn destinations_for(")
        .unwrap_or_else(|| panic!("no `destinations_for` in {}", path.display()));
    let end = src[start..]
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`destinations_for` in {} is unterminated", path.display()))
        + start;

    // Comments are stripped BEFORE the scan: the arms are explained in prose,
    // and good comments restate an invariant in the words the assertion greps
    // for, which would make a well-commented file easier to pass.
    let body: String = src[start..end]
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !body.contains("_ =>") && !body.contains("_ if"),
        "the delivery table has a wildcard arm, so adding a PackageKind is a \
         silent non-delivery rather than a compile error:\n{body}"
    );
    // A body that matched nothing would satisfy the assertion above vacuously.
    for kind in ["Napi", "Pyo3", "Ffi", "Core", "Server", "Wasm", "Transform", "Engine"] {
        assert!(
            body.contains(kind),
            "`{kind}` has no arm in the delivery table — the match is not total"
        );
    }
}

/// **Scenario 11.** Delivery never guesses.
///
/// A report naming a path that does not exist is an error NAMING the path. It
/// does not skip, and — the property that matters — it does not reconstruct a
/// path under `target/`, which is #292's defect one layer down.
#[test]
fn scenario_11_delivery_errors_on_a_reported_path_that_is_not_there() {
    use forgedb::commands::build::deliver;
    use forgedb::commands::build::driver::{BuildReport, Profile, ReportedArtifact, TargetKind};

    let dir = std::env::temp_dir().join(format!("forgedb-deliver-s11-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("napi")).unwrap();

    let ghost = dir.join("nowhere/libapp_napi.dylib");
    let report = BuildReport {
        version: 1,
        project: dir.clone(),
        app: dir.clone(),
        profile: Profile::Release,
        artifacts: vec![ReportedArtifact {
            package: "app-napi".to_string(),
            kind: "napi".to_string(),
            target_kind: TargetKind::Cdylib,
            path: ghost.clone(),
            triple: "aarch64-apple-darwin".to_string(),
        }],
        delivered: Vec::new(),
    };

    let err = deliver::run(&dir, &report)
        .expect_err("delivering a path that is not there must fail")
        .to_string();
    assert!(
        err.contains(&ghost.display().to_string()),
        "the error does not name the missing path: {err}"
    );
    // The message must say WHY this is not a path ForgeDB could have guessed
    // wrong. `fs::copy`'s own ENOENT names the syscall, and a reader who has
    // just been burned by #292 will assume delivery reconstructed the path.
    assert!(
        err.contains("does not reconstruct"),
        "the error does not distinguish a moved file from a guessed path: {err}"
    );
    assert!(
        !dir.join("napi/forgedb.node").exists(),
        "delivery produced a file from a source that does not exist"
    );

    // …and an EMPTY report against an existing destination is the other half:
    // a hard error naming what the build did produce, never a silent skip.
    let empty = BuildReport {
        version: 1,
        project: dir.clone(),
        app: dir.clone(),
        profile: Profile::Release,
        artifacts: Vec::new(),
        delivered: Vec::new(),
    };
    let err = deliver::run(&dir, &empty)
        .expect_err("an existing destination with nothing to deliver must fail")
        .to_string();
    assert!(err.contains("napi"), "the error does not name the target: {err}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// **Scenario 13.** `--check` covers the shims.
#[test]
fn scenario_13_check_mode_sees_an_edited_shim() {
    let dir = project("s13", "\"rust\", \"node-runtime\", \"python-runtime\"");
    ok(&forgedb(&dir, &["generate", "all"]), "generate all");
    ok(&forgedb(&dir, &["generate", "all", "--check"]), "generate --check (clean)");

    for shim in ["generated/napi/index.js", "generated/pyo3/forgedb.py"] {
        let path = dir.join(shim);
        let original = read(&path);
        std::fs::write(&path, format!("{original}// one byte\n")).unwrap();

        let out = forgedb(&dir, &["generate", "all", "--check"]);
        assert!(
            !out.status.success(),
            "`generate --check` passed with an edited {shim}"
        );
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            combined.contains(shim.rsplit('/').next().unwrap()),
            "`--check` did not name the stale file:\n{combined}"
        );
        std::fs::write(&path, original).unwrap();
    }
}

/// **Scenario 14.** The macOS install name is not a dependency.
///
/// `otool -L` prints `LC_ID_DYLIB` first, and for a rustc cdylib it is the
/// ABSOLUTE build directory — so a naive "no cache path anywhere in `otool -L`"
/// assertion fails on macOS and would send napi and pyo3 to the archive rule for
/// a reason that is not a defect.
///
/// A pure function over captured text, so it runs on any host. That is the
/// lesson `parses_both_tools_output` already encodes: a rule written against the
/// output of one host, on a test that only ever ran on that host, is how #409's
/// Linux-only bug survived.
#[test]
fn scenario_14_the_install_name_is_not_a_loaded_library() {
    // Verbatim from a real delivered `forgedb.node`, cache path included.
    let install_name = "/tmp/fp337/home/projects/fp337/target/release/deps/libfp337_napi.dylib";
    let otool = format!(
        "napi/forgedb.node:\n\
         \t{install_name} (compatibility version 0.0.0, current version 0.0.0)\n\
         \t/System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation (compatibility version 150.0.0, current version 5026.5.4)\n\
         \t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1356.0.0)\n"
    );

    let entries = common::parse_linked_libraries(&otool);
    assert_eq!(entries.len(), 3, "the header line must be dropped: {entries:?}");
    assert_eq!(entries[0], install_name);

    // The classification: everything EXCEPT the `otool -D` entry must resolve
    // outside the cache.
    let dependencies: Vec<&String> = entries.iter().filter(|e| *e != install_name).collect();
    assert_eq!(dependencies.len(), 2);
    for dep in dependencies {
        assert!(
            !dep.contains("/.forgedb") && !dep.contains("/projects/"),
            "a genuine dependency reaches into the cache: {dep}"
        );
    }
}

// ===========================================================================
// Tier 2 — the artifact is compiled, delivered, and RUN
// ===========================================================================

/// Generate + build a project, and return its directory.
fn generate_and_build(tag: &str, targets: &str) -> PathBuf {
    let dir = project(tag, targets);
    ok(&forgedb(&dir, &["generate", "all"]), "generate all");
    ok(&forgedb(&dir, &["build"]), "forgedb build");
    dir
}

/// The C8 garbage collection, in full. The deletion IS the test; a run that
/// skips it proves nothing.
fn delete_the_cache(dir: &Path) {
    let home = dir.join(".home");
    assert!(home.is_dir(), "no cache to delete at {}", home.display());
    std::fs::remove_dir_all(&home).unwrap();
}

/// **Scenario 15 ★.** Nothing delivered reaches back into the cache.
///
/// Gate 1's carried verification step, executed rather than asserted about: if
/// either the `.node` or the Python extension shows a genuine dependency into
/// the cache, that target joins the archive rule.
#[test]
#[ignore = "compiles a release cargo workspace"]
fn scenario_15_nothing_delivered_depends_on_the_cache() {
    let dir = generate_and_build("s15", "\"rust\", \"node-runtime\", \"python-runtime\"");

    for delivered in [
        dir.join("generated/napi/forgedb.node"),
        dir.join("generated/pyo3/_forgedb_native.abi3.so"),
    ] {
        assert!(delivered.is_file(), "not delivered: {}", delivered.display());

        // The file's OWN install name, excluded by what it is rather than by
        // position. rustc stamps an absolute build path there and it is not a
        // dependency — scenarios 16 and 17 are the ground truth this only
        // proxies.
        let own = if cfg!(target_os = "macos") {
            let out = Command::new("otool")
                .args(["-D".as_ref(), delivered.as_os_str()])
                .output()
                .expect("otool -D");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| !l.trim_end().ends_with(':'))
                .map(|l| l.trim().to_string())
        } else {
            None
        };

        for lib in linked_libraries(&delivered) {
            if Some(&lib) == own.as_ref() {
                continue;
            }
            assert!(
                !lib.contains("/.home/") && !lib.contains("/.forgedb/"),
                "{} loads a library from inside the build cache: {lib}\n{}",
                delivered.display(),
                load_commands(&delivered)
            );
        }
    }
}

/// **Scenario 16 ★.** Node loads it with the cache deleted, and it works.
#[test]
#[ignore = "compiles a release cargo workspace and runs node"]
fn scenario_16_node_loads_the_delivered_addon_after_the_cache_is_deleted() {
    require_tool("node", "--version");
    let dir = generate_and_build("s16", "\"rust\", \"node-runtime\"");
    delete_the_cache(&dir);

    let script = dir.join("load.js");
    write(
        &script,
        &format!(
            "const db = require({:?});\n\
             const fs = require('fs'), os = require('os'), path = require('path');\n\
             const data = fs.mkdtempSync(path.join(os.tmpdir(), 'forgedb-s16-'));\n\
             const h = db.ForgeDb.open(data);\n\
             h.createAuthor({{ email: 'a@b.c', name: 'Ada', posts: null }});\n\
             h.commit();\n\
             const rows = h.allAuthor();\n\
             if (rows.length !== 1 || rows[0].name !== 'Ada') {{\n\
             \x20 throw new Error('read back the wrong row: ' + JSON.stringify(rows));\n\
             }}\n\
             console.log('OK');\n",
            dir.join("generated/napi").display().to_string()
        ),
    );

    let out = Command::new("node").arg(&script).output().expect("run node");
    assert!(
        out.status.success(),
        "node could not load the delivered addon after the cache was deleted:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("OK"));
}

/// **Scenario 17 ★.** Python imports it with the cache deleted.
///
/// This is the only check that can see a `PyInit_<stem>` mismatch: it is
/// invisible to `cargo build`, to `cargo check` and to every snapshot.
#[test]
#[ignore = "compiles a release cargo workspace and runs python3"]
fn scenario_17_python_imports_the_delivered_extension_after_the_cache_is_deleted() {
    require_tool("python3", "--version");
    let dir = generate_and_build("s17", "\"rust\", \"python-runtime\"");
    delete_the_cache(&dir);

    let script = dir.join("load.py");
    write(
        &script,
        &format!(
            "import sys, tempfile\n\
             sys.path.insert(0, {:?})\n\
             import forgedb\n\
             h = forgedb.ForgeDb.open(tempfile.mkdtemp())\n\
             h.create_author({{'email': 'a@b.c', 'name': 'Ada', 'posts': None}})\n\
             h.commit()\n\
             rows = [r.to_dict() for r in h.all_author()]\n\
             assert len(rows) == 1 and rows[0]['name'] == 'Ada', rows\n\
             print('OK')\n",
            dir.join("generated/pyo3").display().to_string()
        ),
    );

    let out = Command::new("python3").arg(&script).output().expect("run python3");
    assert!(
        out.status.success(),
        "python could not import the delivered extension after the cache was deleted:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("OK"));
}

/// **Scenario 18 ★ — the CALL-SITE mutation.**
///
/// Scenarios 1–4 prove the comparison is correct. Only this proves it *runs*
/// (#345's lesson: mutating a predicate proves the check works, mutating the
/// call site proves it executes). The schema changes, only `generate` runs, and
/// both runtimes must refuse to load with a message naming the remedy.
#[test]
#[ignore = "compiles a release cargo workspace and runs node + python3"]
fn scenario_18_a_stale_artifact_fails_at_load_with_the_remedy() {
    require_tool("node", "--version");
    require_tool("python3", "--version");
    let dir = generate_and_build("s18", "\"rust\", \"node-runtime\", \"python-runtime\"");

    // A schema change, then GENERATE ONLY — the `forgedb dev` shape.
    let schema = dir.join("schema.forge");
    std::fs::write(
        &schema,
        format!("{SCHEMA}\nTag {{\n  id: +uuid\n  label: string\n}}\n"),
    )
    .unwrap();
    ok(&forgedb(&dir, &["generate", "all", "--force"]), "regenerate");

    let js = dir.join("stale.js");
    write(
        &js,
        &format!("require({:?});\n", dir.join("generated/napi").display().to_string()),
    );
    let out = Command::new("node").arg(&js).output().expect("run node");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "node loaded a stale addon:\n{err}");
    assert!(
        err.contains("different schema") && err.contains("forgedb build"),
        "the Node error does not name the mismatch and the remedy:\n{err}"
    );

    let py = dir.join("stale.py");
    write(
        &py,
        &format!(
            "import sys\nsys.path.insert(0, {:?})\nimport forgedb\n",
            dir.join("generated/pyo3").display().to_string()
        ),
    );
    let out = Command::new("python3").arg(&py).output().expect("run python3");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "python imported a stale extension:\n{err}");
    assert!(
        err.contains("different schema") && err.contains("forgedb build"),
        "the Python error does not name the mismatch and the remedy:\n{err}"
    );
}

/// **Scenario 19.** Go's `init()` sees what the linker cannot.
///
/// A `[storage]` knob change alters durability semantics and no exported symbol,
/// so `go build` still succeeds — and the binary must fail at start with the
/// named message. That is the case the linker's undefined-symbol error cannot
/// reach, and the honest description of what this check adds.
#[test]
#[ignore = "compiles a release cargo workspace and a cgo binary"]
fn scenario_19_go_init_catches_a_config_only_change_the_linker_cannot_see() {
    require_tool("go", "version");
    let dir = generate_and_build("s19", "\"rust\", \"go-runtime\"");
    let smoke = build_go_smoke(&dir);
    assert!(
        Command::new(&smoke).output().expect("run smoke").status.success(),
        "the Go smoke binary failed before the config change"
    );

    // Durability semantics change; not one exported symbol does.
    std::fs::write(
        dir.join("forgedb.toml"),
        config("s19", "\"rust\", \"go-runtime\"").replace("fsync = \"never\"", "fsync = \"always\""),
    )
    .unwrap();
    ok(&forgedb(&dir, &["generate", "all", "--force"]), "regenerate");

    let smoke = build_go_smoke(&dir);
    let out = Command::new(&smoke).output().expect("run smoke");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "the Go binary ran against an archive built from different source:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        err.contains("different schema") && err.contains("forgedb build"),
        "the Go panic does not name the mismatch and the remedy:\n{err}"
    );
}

/// Compile the Go smoke consumer against the delivered archive, returning the
/// binary. The Arrow file pulls an external module, so it is dropped — it is
/// orthogonal to how the engine links.
fn build_go_smoke(dir: &Path) -> PathBuf {
    let go_dir = dir.join("generated/go");
    assert!(
        go_dir.join("libforgedb.a").is_file(),
        "`forgedb build` did not deliver the archive to {}",
        go_dir.display()
    );
    let _ = std::fs::remove_file(go_dir.join("forgedb_arrow.go"));
    write(&go_dir.join("go.mod"), "module forgedb\n\ngo 1.21\n");

    let smoke = dir.join("generated/smoke");
    write(
        &smoke.join("go.mod"),
        "module smoke\n\ngo 1.21\n\nrequire forgedb v0.0.0\n\nreplace forgedb => ../go\n",
    );
    write(&smoke.join("main.go"), SMOKE_MAIN);

    let built = Command::new("go")
        .args(["build", "-o", "smoke", "."])
        .current_dir(&smoke)
        .env("CGO_ENABLED", "1")
        .output()
        .expect("go build");
    assert!(
        built.status.success(),
        "go build failed — a config-only change must NOT break the link, which is \
         the whole reason init() has to carry this check:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    smoke.join("smoke")
}

const SMOKE_MAIN: &str = r#"package main

import (
	"fmt"
	"os"

	"forgedb"
)

func main() {
	dir, err := os.MkdirTemp("", "forgedb-go-smoke")
	if err != nil {
		fmt.Fprintln(os.Stderr, "mktemp:", err)
		os.Exit(1)
	}
	defer os.RemoveAll(dir)

	db, err := forgedb.Open(dir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "open:", err)
		os.Exit(1)
	}
	defer db.Close()
	fmt.Println("OK")
}
"#;

/// **Scenario 21 ★.** A C consumer builds against what was delivered, WITH THE
/// CACHE DELETED, and the accessor agrees with the header's macro.
#[test]
#[ignore = "compiles a release cargo workspace and a C program"]
fn scenario_21_a_c_consumer_links_the_delivered_archive_with_the_cache_deleted() {
    require_tool("cc", "--version");
    let dir = generate_and_build("s21", "\"rust\", \"ffi\"");
    let ffi = dir.join("generated/ffi");
    assert!(ffi.join("forgedb.h").is_file(), "no delivered header");
    assert!(ffi.join("libforgedb.a").is_file(), "no delivered archive");

    delete_the_cache(&dir);

    let c = dir.join("smoke.c");
    write(
        &c,
        "#include <stdio.h>\n\
         #include \"forgedb.h\"\n\
         int main(void) {\n\
         \x20 printf(\"%s\\n\", FORGEDB_FINGERPRINT);\n\
         \x20 return forgedb_fingerprint_ok() ? 0 : 1;\n\
         }\n",
    );

    let bin = dir.join("csmoke");
    let mut cmd = Command::new("cc");
    cmd.arg("-I")
        .arg(&ffi)
        .arg(&c)
        .arg(ffi.join("libforgedb.a"));
    if cfg!(target_os = "macos") {
        cmd.args(["-framework", "CoreFoundation", "-framework", "Security", "-lc++"]);
    } else {
        cmd.args(["-lm", "-ldl", "-lpthread", "-lstdc++"]);
    }
    let built = cmd.arg("-o").arg(&bin).output().expect("cc");
    assert!(
        built.status.success(),
        "the C consumer did not build against the delivered archive:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let out = Command::new(&bin).output().expect("run csmoke");
    assert!(
        out.status.success(),
        "the archive's fingerprint accessor disagrees with the header's macro:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// **Scenario 22.** Idempotence: `generate` + `build` again with no schema
/// change leaves `--check` clean and the delivered files byte-identical.
#[test]
#[ignore = "compiles a release cargo workspace"]
fn scenario_22_a_second_generate_and_build_changes_nothing() {
    let dir = generate_and_build("s22", "\"rust\", \"node-runtime\", \"python-runtime\", \"ffi\"");

    let delivered = [
        "generated/napi/forgedb.node",
        "generated/pyo3/_forgedb_native.abi3.so",
        "generated/ffi/libforgedb.a",
    ];
    let before: Vec<Vec<u8>> = delivered
        .iter()
        .map(|p| std::fs::read(dir.join(p)).unwrap_or_else(|e| panic!("{p}: {e}")))
        .collect();

    ok(&forgedb(&dir, &["generate", "all", "--force"]), "regenerate");
    ok(&forgedb(&dir, &["generate", "all", "--check"]), "generate --check");
    ok(&forgedb(&dir, &["build"]), "rebuild");

    for (p, was) in delivered.iter().zip(before) {
        assert_eq!(
            std::fs::read(dir.join(p)).unwrap(),
            was,
            "{p} changed across a no-op regenerate + rebuild"
        );
    }
}

/// **Scenario 20 — C6, and the epic's main drift guard.**
///
/// Two different schemas at ONE app path must produce different exported
/// surfaces **and** different fingerprints. Two assertions, deliberately not
/// collapsed: the fingerprint is over SOURCE, and C6 is about the exported
/// surface. An artifact ForgeDB could build once and hand to everybody cannot be
/// per-schema — it would have to read a schema at runtime, which is the red line.
///
/// This is not `scenario_3_the_two_staticlibs_export_disjoint_symbols`: that one
/// is about two APPS, which is placement. This is about two SCHEMAS.
#[test]
#[ignore = "compiles two release cargo workspaces"]
fn scenario_20_two_schemas_at_one_path_differ_in_symbols_and_in_fingerprint() {
    require_tool("nm", "--version");

    let dir = generate_and_build("s20", "\"rust\", \"ffi\"");
    let archive = dir.join("generated/ffi/libforgedb.a");
    let first_syms = defined_symbols(&archive);
    let first_fp = header_fingerprint(&dir.join("generated/ffi/forgedb.h"));

    // A DIFFERENT schema at the same path: same project, same app, same symbol
    // prefix. Only the models change.
    std::fs::write(
        dir.join("schema.forge"),
        "Widget {\n  id: +uuid\n  sku: &string\n  weight: f64\n}\n",
    )
    .unwrap();
    ok(&forgedb(&dir, &["generate", "all", "--force"]), "regenerate");
    ok(&forgedb(&dir, &["build"]), "rebuild");

    let second_syms = defined_symbols(&archive);
    let second_fp = header_fingerprint(&dir.join("generated/ffi/forgedb.h"));

    // C6: the EXPORTED SURFACE differs. Named symbols, not a count — a count
    // would pass for two schemas with the same number of models.
    let gone: Vec<&String> = first_syms.difference(&second_syms).collect();
    let arrived: Vec<&String> = second_syms.difference(&first_syms).collect();
    assert!(
        !gone.is_empty() && !arrived.is_empty(),
        "the two schemas export the same symbols, so the artifact is not \
         per-schema: {} defined before, {} after",
        first_syms.len(),
        second_syms.len()
    );
    assert!(
        arrived.iter().any(|s| s.contains("widget")),
        "the second schema's model has no symbol of its own: {arrived:?}"
    );

    // …and the FINGERPRINT differs. A separate claim over a separate input.
    assert_ne!(
        first_fp, second_fp,
        "two different schemas produced the same source fingerprint"
    );
}

/// Every defined text symbol in an archive.
///
/// The exit status is deliberately not asserted: Xcode's `nm` reports `Unknown
/// attribute kind` on rustc bitcode and exits 1 while still printing tens of
/// thousands of symbols. Emptiness is the real failure, and it panics.
fn defined_symbols(archive: &Path) -> std::collections::BTreeSet<String> {
    let nm = Command::new("nm").arg("-g").arg(archive).output().expect("nm runs");
    let out: std::collections::BTreeSet<String> = String::from_utf8_lossy(&nm.stdout)
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

/// The `FORGEDB_FINGERPRINT` macro's value. PANICS on a miss rather than
/// degrading to the whole file.
fn header_fingerprint(header: &Path) -> String {
    let src = read(header);
    let key = "#define FORGEDB_FINGERPRINT \"";
    let open = src
        .find(key)
        .unwrap_or_else(|| panic!("no fingerprint macro in {}", header.display()))
        + key.len();
    let close = src[open..]
        .find('"')
        .unwrap_or_else(|| panic!("unterminated fingerprint macro in {}", header.display()));
    src[open..open + close].to_string()
}
