//! Step 7 of #335 — the placement flip and the supersession rule.
//!
//! Three of the plan's (#347) thirty-seven scenarios live here:
//!
//! * **29** `output/database.rs` and `core/src/lib.rs` are identical bytes.
//! * **30** a superseded generated file becomes a `compile_error!`, idempotently,
//!   and nothing else in its directory is touched.
//! * **31** ★ a Go binary survives deletion of the entire cargo target directory.
//!
//! Everything here drives the real `forgedb` binary as a subprocess with an
//! explicit `current_dir` and its own `FORGEDB_HOME`, so the cases are hermetic
//! and run in parallel — the convention `tests/integration_test.rs` and
//! `tests/build_cache_compile_test.rs` already follow.
//!
//! The cheap half of each scenario runs by default. Scenario 31's end-to-end
//! half compiles a release cargo workspace **and** a cgo binary, so it is
//! `#[ignore]`d like the other compiling cases in this suite; the structural
//! half of the same property — what the emitted cgo preamble actually says —
//! runs every time, because that is where the dylib mistake would reappear.

mod common;

use common::{linked_libraries, load_commands, parse_linked_libraries};
use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = r#"
Author {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: ^string
  body: string
  author: *Author
}
"#;

/// A config naming every target the flip moves, plus the two it mirrors.
fn config(name: &str, targets: &str) -> String {
    format!(
        "[project]\nname = \"{name}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n"
    )
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// A fresh project directory with a schema and a `forgedb.toml`.
fn project(tag: &str, targets: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-flip-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("schema.forge"), SCHEMA);
    write(&dir.join("forgedb.toml"), &config(tag, targets));
    dir
}

/// Run the CLI in `dir` with a `FORGEDB_HOME` inside it.
///
/// The override is not hygiene, it is correctness: without it `generate` claims
/// a project id in the developer's real `~/.forgedb` ledger and writes cache
/// packages outside the tempdir.
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
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The app's single container under the cache, found by scanning rather than by
/// recomputing the member hash — a second derivation of the hash in a test is a
/// way for the test to agree with itself while disagreeing with the CLI.
fn container(dir: &Path, name: &str) -> PathBuf {
    let apps = dir.join(".home/projects").join(name).join("apps");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no cache at {}: {e}", apps.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(found.len(), 1, "expected exactly one app container: {found:?}");
    found.pop().unwrap()
}

// ===========================================================================
// Scenario 29 — the mirror is byte-identical to the cache copy
// ===========================================================================

/// **Scenario 29.** `output/database.rs` and `core/src/lib.rs` are identical
/// bytes, and so are `output/api.rs` and `server/src/api.rs`.
///
/// This is a byte compare rather than a substring check on purpose: the defect
/// it replaces was two files that were *mostly* the same — same models, same
/// methods — and differed only in the durability semantics baked into them by
/// two generator invocations with different `GenConfig`. Any assertion weaker
/// than "identical" passes while that bug is present.
#[test]
fn scenario_29_the_output_mirror_is_byte_identical_to_the_cache_copy() {
    let dir = project("s29", "\"rust\", \"api\"");
    ok(&forgedb(&dir, &["generate", "all", "--schema", "schema.forge"]), "generate all");

    let cache = container(&dir, "s29");
    let mirror_db = std::fs::read(dir.join("generated/database.rs")).unwrap();
    let cache_db = std::fs::read(cache.join("core/src/lib.rs")).unwrap();
    assert_eq!(
        mirror_db,
        cache_db,
        "generated/database.rs and core/src/lib.rs are not identical bytes \
         ({} vs {} bytes)",
        mirror_db.len(),
        cache_db.len()
    );
    // Not vacuous: the two files must actually exist and hold the database.
    assert!(
        mirror_db.len() > 10_000,
        "the mirror is suspiciously small ({} bytes) — the compare may be of two empty files",
        mirror_db.len()
    );

    let mirror_api = std::fs::read(dir.join("generated/api.rs")).unwrap();
    let cache_api = std::fs::read(cache.join("server/src/api.rs")).unwrap();
    assert_eq!(mirror_api, cache_api, "api.rs mirror differs from the cache copy");

    // The configured `fsync = "never"` has to be IN the bytes that were
    // compared, or the compare is of two copies of the wrong database. Asserted
    // on the FULLY-QUALIFIED path: `crates/codegen/src/rust.rs` emits
    // unconditional doc comments containing the bare `FsyncPolicy::Always`, so a
    // substring search for the short name passes while the bug is present.
    let text = String::from_utf8(mirror_db).unwrap();
    assert!(
        text.contains("forgedb_wal :: FsyncPolicy :: Never")
            || text.contains("forgedb_wal::FsyncPolicy::Never"),
        "the compared database does not carry the configured fsync policy"
    );
}

/// **Scenario 29, structurally.** `generate/mod.rs` contains exactly ONE call to
/// the Rust database generator.
///
/// The byte compare above proves the two files agree *for this run*. This proves
/// they cannot stop agreeing: one invocation is why. Until #335 there were five
/// — the `rust` arm plus one inside each of the four binding arms — and only the
/// first threaded the app's config, so a single `generate` run produced two
/// databases with different durability semantics.
///
/// Anchored on the CALL token, never on a binding name: a guard anchored on
/// whatever the result happens to be named is a guard a rename silently defeats.
#[test]
fn scenario_29_exactly_one_rust_generator_invocation_exists() {
    let src = include_str!("../src/commands/generate/mod.rs");
    let calls = src.matches("RustGenerator::generate").count();
    assert_eq!(
        calls, 1,
        "expected exactly one `RustGenerator::generate*` call site in generate/mod.rs, found {calls}. \
         A second one is how `generated/database.rs` and the cache copy diverged."
    );
    // Anti-vacuity: the token must be the real thing, inside `ensure_database`.
    let at = src.find("RustGenerator::generate").unwrap();
    let fn_start = src[..at].rfind("fn ensure_database").unwrap_or(usize::MAX);
    assert_ne!(
        fn_start,
        usize::MAX,
        "the single generator call is no longer inside `ensure_database`"
    );
}

/// The four **crates** `output` has stopped receiving are actually gone from it,
/// and the two things that stayed are still there.
///
/// #337 narrowed this from "the directory is absent" to "the CRATE is absent".
/// Those directories now receive the consumer-facing half of each binding — a
/// header, an entry module, a Python module — which is generated text the user
/// commits, not a cargo package. Asserting the whole directory away would refuse
/// the delivery destination the epic exists to create, so the assertion is on the
/// two files that make a directory a crate.
#[test]
fn the_output_directory_no_longer_receives_the_four_moved_packages() {
    let dir = project(
        "moved",
        "\"rust\", \"api\", \"ffi\", \"node-runtime\", \"python-runtime\", \"browser-replica\", \"go-runtime\"",
    );
    ok(&forgedb(&dir, &["generate", "all", "--schema", "schema.forge"]), "generate all");

    let outdir = dir.join("generated");
    for moved in ["ffi", "napi", "pyo3"] {
        for crate_file in ["Cargo.toml", "src/lib.rs", "src/database.rs"] {
            assert!(
                !outdir.join(moved).join(crate_file).exists(),
                "{} still receives `{moved}/{crate_file}`",
                outdir.display()
            );
        }
    }

    // …and each of them DOES receive its consumer-facing half (#337), which is
    // the other half of the same property: the crate moved, the shim arrived.
    assert!(outdir.join("ffi/forgedb.h").is_file(), "no C header in generated/ffi/");
    assert!(outdir.join("napi/index.js").is_file(), "no entry module in generated/napi/");
    assert!(outdir.join("napi/index.d.ts").is_file(), "no declarations in generated/napi/");
    assert!(outdir.join("pyo3/forgedb.py").is_file(), "no Python module in generated/pyo3/");
    assert!(outdir.join("pyo3/forgedb.pyi").is_file(), "no Python stub in generated/pyo3/");
    // `replica/`'s CRATE moved; its browser assets did not — they are files the
    // user's page serves, and a content-hashed cache directory is unservable.
    assert!(!outdir.join("replica/src").exists(), "replica/src/ was not moved");
    assert!(!outdir.join("replica/Cargo.toml").exists(), "replica/Cargo.toml was not moved");
    assert!(outdir.join("replica/client/replica-client.ts").is_file());
    assert!(outdir.join("replica/client/replica-worker.js").is_file());

    // `go/` stays: it is Go source the user's program imports.
    assert!(outdir.join("go/forgedb.go").is_file());
    assert!(outdir.join("go/forgedb.h").is_file());

    // …and every moved package is present in the cache instead.
    let cache = container(&dir, "moved");
    for pkg in ["core", "server", "ffi", "napi", "pyo3", "wasm"] {
        assert!(
            cache.join(pkg).join("Cargo.toml").is_file(),
            "cache is missing the `{pkg}` package"
        );
    }
}

// ===========================================================================
// Scenario 30 — the supersession rule
// ===========================================================================

/// **Scenario 30.** A pre-existing `generated/ffi/src/database.rs` is replaced by
/// a `compile_error!` naming what happened; the path is reported; the
/// user-editable scaffolds beside it are untouched; a second run changes nothing.
///
/// Without this, removing the four packages from `output` leaves four
/// directories frozen, never regenerated, **and still compilable** — in the exact
/// workflow ForgeDB's own Go README and its own reclose tell users to run. Their
/// build keeps going green against a database that stopped tracking the schema.
#[test]
fn scenario_30_a_superseded_file_becomes_a_compile_error_idempotently() {
    let dir = project("s30", "\"rust\", \"api\"");
    let outdir = dir.join("generated");

    // A project left over from before the flip: generated Rust under the moved
    // packages, and the user-editable scaffolds that live beside it.
    write(&outdir.join("ffi/src/database.rs"), "// stale generated database\n");
    write(&outdir.join("ffi/src/lib.rs"), "// stale generated spine\n");
    write(&outdir.join("pyo3/src/database.rs"), "// stale generated database\n");
    write(&outdir.join("pyo3/pyproject.toml"), "[build-system]\nrequires = [\"maturin\"]\n");
    write(&outdir.join("napi/src/lib.rs"), "// stale generated wrapper\n");
    write(&outdir.join("napi/package.json"), "{ \"name\": \"mine\" }\n");
    write(&outdir.join("replica/src/database.rs"), "// stale generated database\n");
    write(&outdir.join("go/go.mod"), "module forgedb\n\ngo 1.21\n");

    let untouched = ["pyo3/pyproject.toml", "napi/package.json", "go/go.mod"];
    let before: Vec<String> = untouched
        .iter()
        .map(|p| std::fs::read_to_string(outdir.join(p)).unwrap())
        .collect();

    let log = ok(
        &forgedb(&dir, &["generate", "all", "--schema", "schema.forge", "--force"]),
        "generate all",
    );

    // The file is a `compile_error!`, and the message names what happened.
    let superseded = std::fs::read_to_string(outdir.join("ffi/src/database.rs")).unwrap();
    assert!(
        superseded.contains("compile_error!"),
        "ffi/src/database.rs was not superseded:\n{superseded}"
    );
    assert!(
        superseded.contains("build cache") && superseded.contains("forgedb build"),
        "the supersession message does not name what happened or what to run:\n{superseded}"
    );
    // Every moved package's generated Rust, not just the one the scenario names.
    for rel in [
        "ffi/src/lib.rs",
        "pyo3/src/database.rs",
        "napi/src/lib.rs",
        "replica/src/database.rs",
    ] {
        assert!(
            std::fs::read_to_string(outdir.join(rel)).unwrap().contains("compile_error!"),
            "{rel} was not superseded"
        );
    }

    // The path is REPORTED — a silent rewrite is the failure mode this replaces.
    assert!(
        log.contains("ffi/src/database.rs"),
        "the superseded path was not reported:\n{log}"
    );

    // Nothing else in those directories moved.
    for (rel, was) in untouched.iter().zip(&before) {
        assert_eq!(
            &std::fs::read_to_string(outdir.join(rel)).unwrap(),
            was,
            "{rel} was modified — supersession must touch only what ForgeDB generated"
        );
    }

    // Idempotent: a second run rewrites nothing and reports nothing.
    let again = ok(
        &forgedb(&dir, &["generate", "all", "--schema", "schema.forge", "--force"]),
        "second generate",
    );
    assert!(
        !again.to_lowercase().contains("superseded"),
        "the second run superseded something again — the content compare is not working:\n{again}"
    );
    assert_eq!(
        std::fs::read_to_string(outdir.join("ffi/src/database.rs")).unwrap(),
        superseded,
        "the second run changed the superseded file"
    );
}

/// Supersession never *creates* a file.
///
/// An absent `generated/napi/src/lib.rs` means a project that never enabled that
/// target. Planting a `compile_error!` there would invent a failure rather than
/// describe one — and would put a broken crate in a directory the user has no
/// reason to look at.
#[test]
fn scenario_30_supersession_never_creates_a_file() {
    let dir = project("s30b", "\"rust\"");
    ok(&forgedb(&dir, &["generate", "all", "--schema", "schema.forge"]), "generate all");
    for rel in ["ffi/src/lib.rs", "napi/src/lib.rs", "pyo3/src/database.rs"] {
        assert!(
            !dir.join("generated").join(rel).exists(),
            "supersession created {rel} out of nothing"
        );
    }
}

// ===========================================================================
// Scenario 31 — the Go binding links statically
// ===========================================================================

/// **Scenario 31, the structural half.** The emitted cgo preamble links the
/// delivered archive out of `${SRCDIR}`, carries per-OS link flags, and names no
/// cargo target directory.
///
/// This runs on every `cargo test` because it is where the dylib mistake would
/// come back, and the end-to-end half below cannot run cheaply. **No snapshot
/// covers this text** — verified: `crates/codegen/tests/` references exactly one
/// Go scaffold function, so a change to `FILE_HEADER` is otherwise unguarded.
#[test]
fn scenario_31_the_go_preamble_links_the_delivered_archive_statically() {
    let dir = project("s31a", "\"go-runtime\"");
    ok(
        &forgedb(&dir, &["generate", "go", "--runtime", "--schema", "schema.forge"]),
        "generate go --runtime",
    );
    let go = std::fs::read_to_string(dir.join("generated/go/forgedb.go")).unwrap();

    assert!(
        go.contains("#cgo LDFLAGS: -L${SRCDIR} -lforgedb"),
        "the cgo preamble does not link the delivered archive:\n{}",
        go.lines().take(40).collect::<Vec<_>>().join("\n")
    );
    // One flag set for both platforms does not link.
    assert!(go.contains("#cgo darwin LDFLAGS:"), "no darwin link flags");
    assert!(go.contains("#cgo linux LDFLAGS:"), "no linux link flags");

    // The three things the flip deletes, each of which would reintroduce a
    // dependency on a directory ForgeDB may garbage-collect.
    assert!(
        !go.contains("../ffi"),
        "the preamble still points at a sibling `ffi/` directory that no longer exists"
    );
    assert!(
        !go.contains("-Wl,-rpath"),
        "an rpath is dead weight against a staticlib, and was cargo-cult for a cdylib whose \
         install name is absolute"
    );
    assert!(
        !go.contains("go:generate"),
        "generated Go still drives cargo itself — a second build driver is what #335 removes"
    );
}

/// The FFI package must offer a `staticlib`, or there is nothing to deliver.
#[test]
fn scenario_31_the_ffi_package_emits_a_staticlib() {
    let dir = project("s31b", "\"go-runtime\"");
    ok(
        &forgedb(&dir, &["generate", "go", "--runtime", "--schema", "schema.forge"]),
        "generate go --runtime",
    );
    let manifest =
        std::fs::read_to_string(container(&dir, "s31b").join("ffi/Cargo.toml")).unwrap();
    assert!(
        manifest.contains("staticlib"),
        "the ffi package does not emit a staticlib:\n{manifest}"
    );
}

/// **Scenario 31 ★, end to end.** Build the Go smoke binary, delete the ENTIRE
/// cargo target directory (the C8 GC, in full), and the binary still runs with
/// only system libraries in its load commands.
///
/// This is the guard that would have caught the dylib mistake. rustc stamps an
/// **absolute** `LC_ID_DYLIB` into a cdylib, so a consumer that linked one
/// records the absolute cache path and dies `dyld: Library not loaded` once the
/// cache is cleared — invisible in CI, because the cache still exists while the
/// reclose runs. The deletion is the whole test; a run that skips it proves
/// nothing.
///
/// `#[ignore]`d: it compiles a release cargo workspace and a cgo binary.
/// Run with `cargo test -p forgedb --test placement_flip_test -- --ignored`.
#[test]
#[ignore = "compiles a release cargo workspace and a cgo binary"]
fn scenario_31_a_go_binary_survives_deletion_of_the_cargo_target_directory() {
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("skipping: no `go` toolchain on PATH");
        return;
    }
    let dir = project("s31e2e", "\"go-runtime\"");
    ok(
        &forgedb(&dir, &["generate", "go", "--runtime", "--schema", "schema.forge"]),
        "generate go --runtime",
    );

    // Compile the app's packages and ask the CLI where the staticlib landed.
    let built = forgedb(&dir, &["build", "--print-artifact", "ffi"]);
    ok(&built, "forgedb build --print-artifact ffi");
    let staticlib = PathBuf::from(String::from_utf8(built.stdout).unwrap().trim().to_string());
    assert!(
        staticlib.is_file(),
        "the reported staticlib does not exist: {}",
        staticlib.display()
    );
    assert_eq!(
        staticlib.extension().and_then(|e| e.to_str()),
        Some("a"),
        "`--print-artifact ffi` reported something that is not an archive: {}",
        staticlib.display()
    );

    // The delivery `forgedb build` already performed (#337 folded the Go
    // carve-out into the general table). Asserted rather than re-run: a test
    // that delivers for itself proves the copy works and says nothing about
    // whether `build` runs it.
    let go_dir = dir.join("generated/go");
    let delivered = go_dir.join("libforgedb.a");
    assert!(
        delivered.is_file(),
        "`forgedb build` did not deliver the archive to {}",
        delivered.display()
    );

    // The Arrow file pulls an external Go module; drop it so this case needs no
    // network. It is orthogonal to what is being asserted (how the engine links).
    let _ = std::fs::remove_file(go_dir.join("forgedb_arrow.go"));
    std::fs::write(go_dir.join("go.mod"), "module forgedb\n\ngo 1.21\n").unwrap();

    let smoke = dir.join("generated/smoke");
    write(
        &smoke.join("go.mod"),
        "module smoke\n\ngo 1.21\n\nrequire forgedb v0.0.0\n\nreplace forgedb => ../go\n",
    );
    write(&smoke.join("main.go"), SMOKE_MAIN);

    let go_build = Command::new("go")
        .args(["build", "-o", "smoke", "."])
        .current_dir(&smoke)
        .env("CGO_ENABLED", "1")
        .output()
        .expect("go build");
    assert!(
        go_build.status.success(),
        "go build failed:\n{}",
        String::from_utf8_lossy(&go_build.stderr)
    );
    let bin = smoke.join("smoke");

    let first = Command::new(&bin).output().expect("run smoke");
    assert!(
        first.status.success(),
        "the smoke binary failed before the GC:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );

    // The C8 garbage collection, in full.
    let target = dir.join(".home/projects/s31e2e/target");
    assert!(target.is_dir(), "no cargo target dir to delete at {}", target.display());
    std::fs::remove_dir_all(&target).unwrap();

    let after = Command::new(&bin).output().expect("run smoke after the GC");
    assert!(
        after.status.success(),
        "the smoke binary did not survive deletion of the cargo target dir \
         (exit {:?}) — the engine is linked dynamically:\n{}",
        after.status.code(),
        String::from_utf8_lossy(&after.stderr)
    );

    // …and it references nothing outside the system.
    for lib in linked_libraries(&bin) {
        assert!(
            lib.starts_with("/usr/lib/")
                || lib.starts_with("/System/")
                || lib.starts_with("/lib/")
                || lib.starts_with("linux-vdso")
                || lib.contains("ld-linux"),
            "the binary loads a non-system library: {lib}\n{}",
            load_commands(&bin)
        );
    }
}

/// Fixtures captured from the real tools, including the exact `ldd` output that failed in
/// CI (run 32549821061).
///
/// NOT `#[ignore]`d: it compiles nothing and shells out to nothing, so it runs in tier 1 on
/// every PR — unlike `scenario_31`, which is nightly-only and, before #409, had only ever
/// been run on macOS. The bug it guards was invisible for exactly that reason.
#[test]
fn parses_both_tools_output() {
    // `otool -L` — a `path:` header, then tab-indented absolute paths.
    let otool = "\
/tmp/smoke/smoke:
\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
\t/usr/lib/libresolv.9.dylib (compatibility version 1.0.0, current version 1.0.0)
";
    assert_eq!(
        parse_linked_libraries(otool),
        vec!["/usr/lib/libSystem.B.dylib", "/usr/lib/libresolv.9.dylib"],
        "otool: the header line must be dropped and each path taken whole"
    );

    // `ldd` — NO header, a pseudo-library with no path, two `soname => path` lines, and the
    // loader as a bare absolute path. Verbatim from the failing CI run.
    let ldd = "\
\tlinux-vdso.so.1 (0x00007fffe2379000)
\tlibgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1 (0x00007f4cff831000)
\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f4cff600000)
\t/lib64/ld-linux-x86-64.so.2 (0x00007f4cff861000)
";
    assert_eq!(
        parse_linked_libraries(ldd),
        vec![
            "linux-vdso.so.1",
            "/lib/x86_64-linux-gnu/libgcc_s.so.1",
            "/lib/x86_64-linux-gnu/libc.so.6",
            "/lib64/ld-linux-x86-64.so.2",
        ],
        "ldd: nothing may be skipped by position, and `soname => path` must yield the PATH"
    );

    // The regression, stated as its own assertion because it is the whole bug: every
    // resolved `ldd` path must satisfy the allow-list scenario_31 applies. Under the old
    // parser these were `libgcc_s.so.1` / `libc.so.6`, which match no prefix.
    for lib in parse_linked_libraries(ldd) {
        assert!(
            lib.starts_with("/lib/")
                || lib.starts_with("/usr/lib/")
                || lib.starts_with("linux-vdso")
                || lib.contains("ld-linux"),
            "a stock Linux binary's `{lib}` must read as a system library"
        );
    }

    // An unresolvable dependency must name the LIBRARY, not the word "not" — otherwise the
    // failure message points at nothing.
    let missing = "\tlibfoo.so.1 => not found\n";
    assert_eq!(parse_linked_libraries(missing), vec!["libfoo.so.1"]);
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

	id, err := db.InsertAuthor(forgedb.Author{Name: "Ada"})
	if err != nil {
		fmt.Fprintln(os.Stderr, "insert:", err)
		os.Exit(1)
	}
	got, err := db.GetAuthor(id)
	if err != nil {
		fmt.Fprintln(os.Stderr, "get:", err)
		os.Exit(1)
	}
	if got == nil || got.Name != "Ada" {
		fmt.Fprintf(os.Stderr, "assert failed: %+v\n", got)
		os.Exit(1)
	}
	fmt.Printf("OK round-trip: id=%s name=%s\n", got.Id, got.Name)
}
"#;
