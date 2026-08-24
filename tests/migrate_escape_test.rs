//! The escape bridge, end to end: a transform written in the author's OWN
//! language, running on the interpreter they already have (#374 direction C).
//!
//! # Why this is not a snapshot test
//!
//! What is under test is a *conversation between two processes*. The generated
//! hop writes one JSON line and blocks on the reply; the author's runtime reads
//! it, transforms it, and writes one line back. Every failure mode that matters
//! here — a buffered stdout that deadlocks, a child that exits early, a reply
//! that decodes into the wrong shape — is invisible to anything that compares
//! generated code as strings, because the emitted text is identical in the
//! broken and working cases. Only running it can see them.
//!
//! # It builds the transformer from `TransformGenerator`, not from `migrate build`
//!
//! Deliberately, and for a stated reason. `migrate build` emits a crate whose
//! substrate deps are pinned from **crates.io**, so a test that shelled out to
//! it would be red for the whole of any cycle carrying a publish gap — which is
//! why `test_migrate_build_reports_the_path_cargo_actually_wrote` is the single
//! `--skip` in `make test-ignored` and `tests/ci_gate_test.rs` asserts that skip
//! matches *exactly one* test. A second registry-dependent ignored test cannot
//! be added without breaking that invariant. So this assembles the same
//! generated sources against **path** deps: identical mechanism, different
//! resolution.
//!
//! # The interpreter
//!
//! `python3`, and its absence is a **hard failure, never a skip**: a guard that
//! skips reports green because it never evaluated. It is present on every
//! GitHub runner and on this machine.
//!
//! ```bash
//! cargo test --test migrate_escape_test -- --ignored --nocapture
//! ```

#[allow(dead_code)]
mod common;

use forgedb_codegen::{
    EscapeBridge, HopPlan, ModelOp, TransformGenerator, TransformPlan, VersionSchema,
};
use forgedb_parser::Schema;
use std::path::{Path, PathBuf};
use std::process::Command;

const V1: &str = "Post {\n  id: +uuid\n  title: string\n  views: u32\n}\n";
const V2: &str = "Post {\n  id: +uuid\n  title: string\n  views: string\n}\n";

fn parse(src: &str) -> Schema {
    forgedb_parser::Parser::new(src)
        .and_then(|mut p| p.parse())
        .expect("fixture schema parses")
}

/// `python3`, or a hard failure naming why this is not a skip.
fn python3() -> String {
    let out = Command::new("python3").arg("--version").output();
    match out {
        Ok(o) if o.status.success() => "python3".to_string(),
        _ => panic!(
            "python3 is required to run the escape bridge end to end and was not found. \
             This is a FAILURE, not a skip: a guard that skips reports green because it \
             never evaluated."
        ),
    }
}

/// A scratch dir outside every project dir (`assert_driver_ok` removes those).
fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("forgedb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Writes three v1 `Post` rows into `argv[1]`.
const WRITE_V1: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let mut db = Database::open_at(dir);
    for (i, (title, views)) in [("alpha", 1u32), ("beta", 22), ("gamma", 333)].iter().enumerate() {
        db.create_post(Post {
            id: format!("00000000-0000-0000-0000-00000000000{}", i + 1)
                .parse()
                .expect("parse uuid"),
            title: title.to_string(),
            views: *views,
        })
        .expect("create post");
    }
    println!("wrote 3 v1 rows");
}
"##;

/// Reads `argv[1]` through the v2 schema and asserts every row's `views` is the
/// DECIMAL STRING of its old number — which is what the author's transform did.
const READ_V2: &str = r##"mod database;
use database::*;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let db = Database::open_at(dir);
    let mut rows = db.post.all();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(rows.len(), 3, "expected the three migrated rows, got {:?}", rows);
    let got: Vec<(String, String)> =
        rows.iter().map(|r| (r.title.clone(), r.views.clone())).collect();
    println!("migrated rows: {:?}", got);
    assert_eq!(
        got,
        vec![
            ("alpha".to_string(), "1".to_string()),
            ("beta".to_string(), "22".to_string()),
            ("gamma".to_string(), "333".to_string()),
        ],
        "the author's own runtime did not produce these values — a per-row copy of \
         each row's OWN number is the point; a constant would give three equal values"
    );
    println!("ESCAPE OK: every row's views is the decimal string of its old number");
}
"##;

/// Build a transformer crate for the v1 -> v2 hop and return its binary.
///
/// `escape` is the bridge to bake in; `field_adds` lets a caller build the
/// deliberately-broken plan scenario 15 needs.
fn build_transformer(
    tag: &str,
    escape: Option<EscapeBridge>,
    model_ops: Vec<ModelOp>,
) -> (PathBuf, PathBuf) {
    let v1: &'static Schema = Box::leak(Box::new(parse(V1)));
    let v2: &'static Schema = Box::leak(Box::new(parse(V2)));

    let plan = TransformPlan {
        versions: vec![
            VersionSchema { version: 1, schema: v1 },
            VersionSchema { version: 2, schema: v2 },
        ],
        hops: vec![HopPlan {
            from_version: 1,
            to_version: 2,
            migration_id: "m1".to_string(),
            model_ops,
            authored_src: None,
            escape,
        }],
    };
    let name = format!("forgedb-{tag}-transformer");
    let crate_out = TransformGenerator::generate(&plan, &name).expect("generate transformer");

    let proj = std::env::temp_dir().join(format!("forgedb-{tag}-proj-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();
    common::write(
        &proj.join("Cargo.toml"),
        &common::path_dep_cargo_toml(&name),
    );
    for (rel, content) in &crate_out.sources {
        common::write(&proj.join(rel), content);
    }

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run cargo build");
    assert!(
        build.status.success(),
        "the GENERATED transformer did not compile — a snapshot pass is not \
         evidence that it does:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    (target.join(format!("debug/{name}")), proj)
}

/// Compile and run a driver against a schema generated at a SPECIFIC schema
/// serial, and return its output.
///
/// `common::generate_compile_run_in` shells out to `forgedb generate`, which
/// derives `EXPECTED_SCHEMA_VERSION` from the project's lineage — and a fixture
/// with no `migrations/` is at the baseline, v1. The transformer stamps its
/// destination v2, so a reader built that way refuses to open it, correctly and
/// unhelpfully. This bakes the version the transformer wrote, through the very
/// same generator call the transformer's own `v2` module comes from.
fn run_driver_at_version(tag: &str, schema_src: &str, version: u32, driver: &str, data: &Path) {
    let schema = parse(schema_src);
    let code = forgedb_codegen::RustGenerator::generate_with_schema_version(&schema, version)
        .expect("generate the versioned database")
        .code;
    let name = format!("forgedb-{tag}");
    let proj = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    common::write(&proj.join("Cargo.toml"), &common::path_dep_cargo_toml(&name));
    common::write(&proj.join("src/database.rs"), &code);
    common::write(&proj.join("src/main.rs"), driver);

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("cargo build");
    assert!(
        build.status.success(),
        "the v{version} reader did not compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let out = Command::new(target.join(format!("debug/{name}")))
        .arg(data)
        .output()
        .expect("run the reader");
    println!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "the migrated rows are not what the transform produced:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&proj);
}

/// The structural ops a real `u32 -> string` hop emits: none. The value change
/// is entirely the author's transform.
fn no_ops() -> Vec<ModelOp> {
    vec![]
}

/// Write an author's Python transform plus ForgeDB's host loop into `dir`.
fn write_escape_script(dir: &Path, body: &str) -> PathBuf {
    let (host_name, host_src) = forgedb_codegen::python_host();
    common::write(&dir.join(&host_name), &host_src);
    let (v1_name, v1_src) = forgedb_codegen::python_types(&parse(V1), 1);
    common::write(&dir.join(&v1_name), &v1_src);
    let (v2_name, v2_src) = forgedb_codegen::python_types(&parse(V2), 2);
    common::write(&dir.join(&v2_name), &v2_src);
    let script = dir.join("transform.py");
    common::write(&script, body);
    script
}

// ---------------------------------------------------------------------------
// Scenario 18 — the escape runs the author's own runtime, and the rows change
// ---------------------------------------------------------------------------

const GOOD_TRANSFORM: &str = r#"from host import run_transform, Row
import v1
import v2


def transform(model: str, row: Row) -> Row:
    if model == "Post":
        source: v1.Post = row  # type: ignore[assignment]
        return {**source, "views": str(source["views"])}  # type: ignore[return-value]
    return row


if __name__ == "__main__":
    run_transform(transform)
"#;

#[test]
#[ignore = "generates and compiles two crates and runs a real interpreter; run with --ignored"]
fn scenario_18_the_escape_runs_the_authors_runtime_and_transforms_the_rows() {
    let python = python3();
    let shared = scratch("escape18");
    let src_data = shared.join("src-data");

    // 1. Three v1 rows.
    let (out, proj) = common::generate_compile_run_in("esc18writer", V1, WRITE_V1, Some(&src_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    // 2. The author's transform, in their own language.
    let script = write_escape_script(&shared.join("escape"), GOOD_TRANSFORM);

    // 3. The generated transformer, with the bridge baked in.
    let (bin, tproj) = build_transformer(
        "esc18",
        Some(EscapeBridge {
            program: python.clone(),
            args: vec![script.display().to_string()],
        }),
        no_ops(),
    );

    let dst_data = shared.join("dst-data");
    let run = Command::new(&bin)
        .arg(&src_data)
        .arg(&dst_data)
        .output()
        .expect("run the transformer");
    println!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        run.status.success(),
        "the transformer failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&tproj);

    // 4. Read the destination back through a v2 app and assert the VALUES.
    run_driver_at_version("esc18reader", V2, 2, READ_V2, &dst_data);

    let _ = std::fs::remove_dir_all(&shared);
}

// ---------------------------------------------------------------------------
// Scenario 19 — a failing escape fails the hop and leaves the source untouched
// ---------------------------------------------------------------------------

const THROWING_TRANSFORM: &str = r#"from host import run_transform, Row

_seen = 0


def transform(model: str, row: Row) -> Row:
    global _seen
    _seen += 1
    if _seen == 2:
        raise RuntimeError("deliberate failure on the second row")
    return {**row, "views": str(row["views"])}


if __name__ == "__main__":
    run_transform(transform)
"#;

/// A transform that dies mid-run fails the WHOLE hop, surfaces the child's own
/// error, publishes nothing, and leaves the source dir byte-identical.
///
/// The last two are the load-bearing assertions. A partial publish would be a
/// data directory that is neither v1 nor v2, and a mutated source would remove
/// the operator's only rollback.
#[test]
#[ignore = "generates and compiles two crates and runs a real interpreter; run with --ignored"]
fn scenario_19_a_failing_escape_fails_the_hop_and_leaves_the_source_untouched() {
    let python = python3();
    let shared = scratch("escape19");
    let src_data = shared.join("src-data");

    let (out, proj) = common::generate_compile_run_in("esc19writer", V1, WRITE_V1, Some(&src_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    let before = fingerprint(&src_data);
    assert!(!before.is_empty(), "the source dir has files to compare");

    let script = write_escape_script(&shared.join("escape"), THROWING_TRANSFORM);
    let (bin, tproj) = build_transformer(
        "esc19",
        Some(EscapeBridge {
            program: python,
            args: vec![script.display().to_string()],
        }),
        no_ops(),
    );

    let dst_data = shared.join("dst-data");
    let run = Command::new(&bin)
        .arg(&src_data)
        .arg(&dst_data)
        .output()
        .expect("run the transformer");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    println!("{log}");

    assert!(!run.status.success(), "a dying transform must fail the hop:\n{log}");
    assert!(
        log.contains("deliberate failure on the second row"),
        "the CHILD's own error must reach the operator — otherwise a failing \
         transform is a migration that failed for no stated reason:\n{log}"
    );
    assert!(
        !dst_data.exists(),
        "the destination must not be published: a partial dir is neither v1 nor v2"
    );
    assert_eq!(
        fingerprint(&src_data),
        before,
        "the source dir is the operator's only rollback: every byte of ROW DATA \
         must be exactly as it was"
    );

    let _ = std::fs::remove_dir_all(&tproj);
    let _ = std::fs::remove_dir_all(&shared);
}

/// Every ROW DATA file under `dir` as `(relative path, length, bytes hash)`,
/// sorted.
///
/// `manifest.json` is deliberately excluded, and the exclusion is the honest
/// scope of the claim rather than a convenience. The transformer opens the
/// source through `vN::Database::open_at`, which normalizes the manifest — a
/// reopen rewrites it whether or not the hop then fails. What the operator's
/// rollback actually depends on is the columns, the tombstones and the WAL, and
/// those are what is compared. Including the manifest would make this test fail
/// on a successful *read*, which says nothing about the failure under test.
fn fingerprint(dir: &Path) -> Vec<(String, u64, u64)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, u64, u64)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else if p.file_name().is_some_and(|n| n == "manifest.json") {
                continue;
            } else if let Ok(bytes) = std::fs::read(&p) {
                // FNV-1a, so this file needs no dependency to say "these bytes".
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for b in &bytes {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x1000_0000_01b3);
                }
                out.push((
                    p.strip_prefix(base).unwrap().display().to_string(),
                    bytes.len() as u64,
                    h,
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Scenarios 13 and 15 — an unanswered required field FAILS, naming itself
// ---------------------------------------------------------------------------

/// The composition of gate 1's decisions 4 and 5, asserted by running it.
///
/// Decision 4 (the build refuses an unanswered hop) is a hash comparison, and
/// gate 2 states its honest limit: hash equality proves *untouched*, not
/// *answered*. An author who deletes the scaffold's `// TODO:` lines and changes
/// nothing else passes it.
///
/// Decision 5 is what covers that case, and this is the test of it. With the
/// defensive type-zero removed, a required field the hop supplies no value for
/// is **absent from the row**, so the destination decode fails NAMING it —
/// instead of the row receiving `""` and the transformer exiting 0. The defect's
/// whole signature is a successful exit, so only running it can see this.
///
/// The plan is constructed by hand, deliberately bypassing
/// `refuse_unanswered_hops`: that is the point — this asserts what happens when
/// the first mechanism does not fire.
#[test]
#[ignore = "generates and compiles two crates; run with --ignored"]
fn scenario_15_an_unanswered_required_field_errors_rather_than_writing_an_empty_string() {
    let shared = scratch("escape15");
    let src_data = shared.join("src-data");

    let (out, proj) = common::generate_compile_run_in("esc15writer", V1, WRITE_V1, Some(&src_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    // v2 adds a REQUIRED `slug` with no default and no answer, so the hop emits
    // no op for it and the key is absent from every row.
    let v1: &'static Schema = Box::leak(Box::new(parse(V1)));
    let v2: &'static Schema = Box::leak(Box::new(parse(
        "Post {\n  id: +uuid\n  title: string\n  views: u32\n  slug: string\n}\n",
    )));
    let plan = TransformPlan {
        versions: vec![
            VersionSchema { version: 1, schema: v1 },
            VersionSchema { version: 2, schema: v2 },
        ],
        hops: vec![HopPlan {
            from_version: 1,
            to_version: 2,
            migration_id: "m1".to_string(),
            model_ops: vec![ModelOp {
                model: "Post".to_string(),
                source_model: "Post".to_string(),
                field_renames: vec![],
                field_removes: vec![],
                // EMPTY. This is what `lower_fill` produces for a required add
                // with no default and no answer.
                field_adds: vec![],
                field_copies: vec![],
                field_null_fills: vec![],
            }],
            authored_src: None,
            escape: None,
        }],
    };
    let name = "forgedb-esc15-transformer";
    let crate_out = TransformGenerator::generate(&plan, name).expect("generate");
    let tproj = std::env::temp_dir().join(format!("forgedb-esc15-proj-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tproj);
    common::write(&tproj.join("Cargo.toml"), &common::path_dep_cargo_toml(name));
    for (rel, content) in &crate_out.sources {
        common::write(&tproj.join(rel), content);
    }
    let target = tproj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&tproj)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("cargo build");
    assert!(
        build.status.success(),
        "the generated transformer must still COMPILE — the failure under test is \
         a runtime one:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let dst = shared.join("dst-data");
    let run = Command::new(target.join(format!("debug/{name}")))
        .arg(&src_data)
        .arg(&dst)
        .output()
        .expect("run the transformer");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    println!("{log}");

    assert!(
        !run.status.success(),
        "an unanswered required field must FAIL the hop. The defect's whole \
         signature is a successful exit, so this assertion is the test:\n{log}"
    );
    assert!(
        log.contains("slug"),
        "the failure must NAME the field. `missing field `slug`` is what makes \
         this actionable; a type-zero would have produced no message at all:\n{log}"
    );
    assert!(
        !dst.exists(),
        "nothing is published when a hop fails"
    );

    let _ = std::fs::remove_dir_all(&tproj);
    let _ = std::fs::remove_dir_all(&shared);
}

// ---------------------------------------------------------------------------
// Scenario 20 — the emitted modules are real code
// ---------------------------------------------------------------------------

/// The generated Python modules and the scaffold that imports them **compile**.
///
/// `python -m compileall` is stdlib, so this needs nothing installed. It catches
/// the class of break a string snapshot cannot: a type expression that renders
/// plausibly and does not parse.
#[test]
#[ignore = "runs a real interpreter; run with --ignored"]
fn scenario_20_the_generated_python_modules_compile() {
    let python = python3();
    let dir = scratch("escape20");
    write_escape_script(&dir, GOOD_TRANSFORM);

    let out = Command::new(&python)
        .args(["-m", "compileall", "-q"])
        .arg(&dir)
        .output()
        .expect("run compileall");
    assert!(
        out.status.success(),
        "the emitted Python did not compile:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // And the control: a scaffold with a syntax error DOES fail, so the
    // assertion above is not vacuous.
    let broken = scratch("escape20broken");
    write_escape_script(&broken, "def transform(model, row)\n    return row\n");
    let out = Command::new(&python)
        .args(["-m", "compileall", "-q"])
        .arg(&broken)
        .output()
        .expect("run compileall");
    assert!(
        !out.status.success(),
        "compileall accepted a syntax error, so it proves nothing about the \
         generated modules"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&broken);
}
