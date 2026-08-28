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

fn config(name: &str, targets: &str) -> String {
    format!(
        "[project]\nid = \"{name}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n"
    )
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn project(tag: &str, targets: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-flip-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("schema.forge"), SCHEMA);
    write(&dir.join("forgedb.toml"), &config(tag, targets));
    dir
}

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
    assert!(
        mirror_db.len() > 10_000,
        "the mirror is suspiciously small ({} bytes) — the compare may be of two empty files",
        mirror_db.len()
    );

    let mirror_api = std::fs::read(dir.join("generated/api.rs")).unwrap();
    let cache_api = std::fs::read(cache.join("server/src/api.rs")).unwrap();
    assert_eq!(mirror_api, cache_api, "api.rs mirror differs from the cache copy");

    let text = String::from_utf8(mirror_db).unwrap();
    assert!(
        text.contains("forgedb_wal :: FsyncPolicy :: Never")
            || text.contains("forgedb_wal::FsyncPolicy::Never"),
        "the compared database does not carry the configured fsync policy"
    );
}

#[test]
fn scenario_29_exactly_one_rust_generator_invocation_exists() {
    let src = include_str!("../src/commands/generate/mod.rs");
    let calls = src.matches("RustGenerator::generate").count();
    assert_eq!(
        calls, 1,
        "expected exactly one `RustGenerator::generate*` call site in generate/mod.rs, found {calls}. \
         A second one is how `generated/database.rs` and the cache copy diverged."
    );
    let at = src.find("RustGenerator::generate").unwrap();
    let fn_start = src[..at].rfind("fn ensure_database").unwrap_or(usize::MAX);
    assert_ne!(
        fn_start,
        usize::MAX,
        "the single generator call is no longer inside `ensure_database`"
    );
}

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

    assert!(outdir.join("ffi/forgedb.h").is_file(), "no C header in generated/ffi/");
    assert!(outdir.join("napi/index.js").is_file(), "no entry module in generated/napi/");
    assert!(outdir.join("napi/index.d.ts").is_file(), "no declarations in generated/napi/");
    assert!(outdir.join("pyo3/forgedb.py").is_file(), "no Python module in generated/pyo3/");
    assert!(outdir.join("pyo3/forgedb.pyi").is_file(), "no Python stub in generated/pyo3/");
    assert!(!outdir.join("replica/src").exists(), "replica/src/ was not moved");
    assert!(!outdir.join("replica/Cargo.toml").exists(), "replica/Cargo.toml was not moved");
    assert!(outdir.join("replica/client/replica-client.ts").is_file());
    assert!(outdir.join("replica/client/replica-worker.js").is_file());

    assert!(outdir.join("go/forgedb.go").is_file());
    assert!(outdir.join("go/forgedb.h").is_file());

    let cache = container(&dir, "moved");
    for pkg in ["core", "server", "ffi", "napi", "pyo3", "wasm"] {
        assert!(
            cache.join(pkg).join("Cargo.toml").is_file(),
            "cache is missing the `{pkg}` package"
        );
    }
}

#[test]
fn scenario_30_a_superseded_file_becomes_a_compile_error_idempotently() {
    let dir = project("s30", "\"rust\", \"api\"");
    let outdir = dir.join("generated");

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

    let superseded = std::fs::read_to_string(outdir.join("ffi/src/database.rs")).unwrap();
    assert!(
        superseded.contains("compile_error!"),
        "ffi/src/database.rs was not superseded:\n{superseded}"
    );
    assert!(
        superseded.contains("build cache") && superseded.contains("forgedb build"),
        "the supersession message does not name what happened or what to run:\n{superseded}"
    );
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

    assert!(
        log.contains("ffi/src/database.rs"),
        "the superseded path was not reported:\n{log}"
    );

    for (rel, was) in untouched.iter().zip(&before) {
        assert_eq!(
            &std::fs::read_to_string(outdir.join(rel)).unwrap(),
            was,
            "{rel} was modified — supersession must touch only what ForgeDB generated"
        );
    }

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
    assert!(go.contains("#cgo darwin LDFLAGS:"), "no darwin link flags");
    assert!(go.contains("#cgo linux LDFLAGS:"), "no linux link flags");

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

    let go_dir = dir.join("generated/go");
    let delivered = go_dir.join("libforgedb.a");
    assert!(
        delivered.is_file(),
        "`forgedb build` did not deliver the archive to {}",
        delivered.display()
    );

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

#[test]
fn parses_both_tools_output() {
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

    for lib in parse_linked_libraries(ldd) {
        assert!(
            lib.starts_with("/lib/")
                || lib.starts_with("/usr/lib/")
                || lib.starts_with("linux-vdso")
                || lib.contains("ld-linux"),
            "a stock Linux binary's `{lib}` must read as a system library"
        );
    }

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
