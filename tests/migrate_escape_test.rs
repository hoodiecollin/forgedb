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

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("forgedb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

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

fn no_ops() -> Vec<ModelOp> {
    vec![]
}

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

    let (out, proj) = common::generate_compile_run_in("esc18writer", V1, WRITE_V1, Some(&src_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    let script = write_escape_script(&shared.join("escape"), GOOD_TRANSFORM);

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

    run_driver_at_version("esc18reader", V2, 2, READ_V2, &dst_data);

    let _ = std::fs::remove_dir_all(&shared);
}

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

#[test]
#[ignore = "generates and compiles two crates; run with --ignored"]
fn scenario_15_an_unanswered_required_field_errors_rather_than_writing_an_empty_string() {
    let shared = scratch("escape15");
    let src_data = shared.join("src-data");

    let (out, proj) = common::generate_compile_run_in("esc15writer", V1, WRITE_V1, Some(&src_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

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
