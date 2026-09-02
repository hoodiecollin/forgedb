use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn dep(name: &str, crate_dir: &str) -> String {
    let path = repo_root().join("crates").join(crate_dir);
    format!("{name} = {{ path = {:?} }}\n", path.to_string_lossy())
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

const SCHEMA: &str = r#"Sample {
  id: +uuid
  bucket: ^string
  score: ^f64
  opt: ^f64?

  @index(bucket, score)
}

Uniq {
  id: +uuid
  serial: &f64
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make f64-index-key`)"]
fn f64_index_keys_are_a_total_order() {
    let proj = std::env::temp_dir().join(format!("forgedb-f64key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), SCHEMA);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let gen_status = Command::new(forgedb)
        .args(["generate", "rust", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .env("FORGEDB_HOME", proj.join(".forgedb-home"))
        .status()
        .expect("run forgedb generate");
    assert!(gen_status.success(), "forgedb generate rust failed");

    let mut cargo_toml = String::from(
        "[package]\nname = \"f64driver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    );
    for (n, d) in [
        ("forgedb-storage", "storage"),
        ("forgedb-types", "types"),
        ("forgedb-changefeed", "changefeed"),
        ("forgedb-wal", "wal"),
        ("forgedb-compaction", "compaction"),
        ("forgedb-txn", "txn"),
        ("forgedb-coordinator", "coordinator"),
    ] {
        cargo_toml.push_str(&dep(n, d));
    }
    cargo_toml.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
    cargo_toml.push_str("serde_json = \"1\"\n");
    cargo_toml.push_str("rust_decimal = { version = \"1\", features = [\"serde-with-str\"] }\n");
    cargo_toml.push_str("utoipa = { version = \"5\", features = [\"uuid\"] }\n");
    cargo_toml.push_str("\n[workspace]\n");
    write(&proj.join("Cargo.toml"), &cargo_toml);

    write(&proj.join("src/main.rs"), DRIVER);

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("run cargo build");
    assert!(build.success(), "driver failed to compile");

    let out = Command::new(target.join("debug/f64driver"))
        .arg(proj.join("data"))
        .output()
        .expect("run driver");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(
        out.status.success(),
        "driver reported a failure:\n{stdout}\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&proj);
}

const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::Uuid;

static mut FAILURES: u32 = 0;

fn ok(label: &str) {
    println!("ok    {label}");
}
fn bad(label: &str, detail: String) {
    println!("FAIL  {label}: {detail}");
    unsafe { FAILURES += 1 };
}

fn only(label: &str, rows: Vec<Sample>, want: Uuid) {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if ids == vec![want] {
        ok(label);
    } else {
        bad(label, format!("expected exactly [{want}], got {ids:?}"));
    }
}

fn ids_of(rows: &[Sample]) -> Vec<Uuid> {
    rows.iter().map(|r| r.id).collect()
}

fn other_nan() -> f64 {
    f64::from_bits(0xfff8_0000_0000_0001)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir"));
    let mut db = Database::open_at(dir);

    let mk = |bucket: &str, score: f64, opt: Option<f64>| Sample {
        id: Uuid::nil(),
        bucket: bucket.to_string(),
        score,
        opt,
    };

    let id_nan = db.create_sample(mk("a", f64::NAN, None)).expect("nan");
    let id_pinf = db.create_sample(mk("a", f64::INFINITY, None)).expect("+inf");
    let id_ninf = db.create_sample(mk("a", f64::NEG_INFINITY, None)).expect("-inf");
    let id_negzero = db.create_sample(mk("a", -0.0, None)).expect("-0.0");
    let id_one = db.create_sample(mk("a", 1.0, None)).expect("1.0");
    let id_negone = db.create_sample(mk("a", -1.0, None)).expect("-1.0");
    let id_optnan = db.create_sample(mk("b", 2.0, Some(f64::NAN))).expect("opt nan");
    let id_optnone = db.create_sample(mk("b", 3.0, None)).expect("opt none");
    let id_othernan = db.create_sample(mk("c", other_nan(), None)).expect("other nan");

    let none_rows = db.sample.find_by_opt(None);
    if none_rows.iter().any(|r| r.id == id_optnan) {
        bad("opt(None) excludes a NaN value", format!("got {:?}", ids_of(&none_rows)));
    } else if none_rows.iter().any(|r| r.id == id_optnone) {
        ok("opt(None) excludes a NaN value");
    } else {
        bad("opt(None) excludes a NaN value", "unset row missing entirely".into());
    }
    only("opt(NaN) finds only the NaN row", db.sample.find_by_opt(Some(f64::NAN)), id_optnan);

    let mut nan_hits = ids_of(&db.sample.find_by_score(f64::NAN));
    nan_hits.sort();
    let mut both_nans = vec![id_nan, id_othernan];
    both_nans.sort();
    if nan_hits == both_nans {
        ok("score(NaN) returns the NaN rows and only those");
    } else {
        bad("score(NaN) returns the NaN rows and only those", format!("expected {both_nans:?}, got {nan_hits:?}"));
    }
    only("score(+Inf)", db.sample.find_by_score(f64::INFINITY), id_pinf);
    only("score(-Inf)", db.sample.find_by_score(f64::NEG_INFINITY), id_ninf);

    only("score(0.0) finds the -0.0 row", db.sample.find_by_score(0.0), id_negzero);

    let mut nan_rows = ids_of(&db.sample.find_by_score(other_nan()));
    nan_rows.sort();
    if nan_rows == both_nans {
        ok("NaN payloads fold to one key");
    } else {
        bad("NaN payloads fold to one key", format!("expected {both_nans:?}, got {nan_rows:?}"));
    }

    let asc = ids_of(&db.sample.find_by_score_range(None, None, false, None));
    let want_asc = vec![id_ninf, id_negone, id_negzero, id_one, id_pinf];
    let got: Vec<Uuid> = asc.iter().copied().filter(|i| want_asc.contains(i)).collect();
    if got == want_asc {
        ok("range(unbounded) orders -Inf < -1.0 < -0.0 < 1.0 < +Inf");
    } else {
        bad(
            "range(unbounded) orders -Inf < -1.0 < -0.0 < 1.0 < +Inf",
            format!("expected {want_asc:?}, got {got:?}"),
        );
    }
    if asc.last() == Some(&id_othernan) || asc.last() == Some(&id_nan) {
        ok("NaN sorts last");
    } else {
        bad("NaN sorts last", format!("range ended on {:?}", asc.last()));
    }

    let bounded = ids_of(&db.sample.find_by_score_range(Some(0.0), Some(1.0), false, None));
    let want_bounded = vec![id_negzero, id_one];
    if bounded == want_bounded {
        ok("range(0.0, 1.0) excludes NaN and both infinities");
    } else {
        bad(
            "range(0.0, 1.0) excludes NaN and both infinities",
            format!("expected {want_bounded:?}, got {bounded:?}"),
        );
    }
    let desc = ids_of(&db.sample.find_by_score_range(Some(0.0), Some(1.0), true, None));
    if desc == vec![id_one, id_negzero] {
        ok("range descending reverses");
    } else {
        bad("range descending reverses", format!("got {desc:?}"));
    }

    only(
        "composite (bucket, score) with a NaN component",
        db.sample.find_by_bucket_and_score("a", f64::NAN),
        id_nan,
    );
    only(
        "composite (bucket, score) with a +Inf component",
        db.sample.find_by_bucket_and_score("a", f64::INFINITY),
        id_pinf,
    );

    let u_pinf = db.create_uniq(Uniq { id: Uuid::nil(), serial: f64::INFINITY });
    let u_ninf = db.create_uniq(Uniq { id: Uuid::nil(), serial: f64::NEG_INFINITY });
    let u_nan = db.create_uniq(Uniq { id: Uuid::nil(), serial: f64::NAN });
    match (&u_pinf, &u_ninf, &u_nan) {
        (Ok(_), Ok(_), Ok(_)) => ok("&f64 accepts +Inf, -Inf and NaN as distinct values"),
        _ => bad(
            "&f64 accepts +Inf, -Inf and NaN as distinct values",
            format!("+inf={:?} -inf={:?} nan={:?}", u_pinf.is_ok(), u_ninf.is_ok(), u_nan.is_ok()),
        ),
    }
    if db.create_uniq(Uniq { id: Uuid::nil(), serial: f64::INFINITY }).is_err() {
        ok("&f64 still rejects a duplicate +Inf");
    } else {
        bad("&f64 still rejects a duplicate +Inf", "second insert succeeded".into());
    }
    if db.create_uniq(Uniq { id: Uuid::nil(), serial: other_nan() }).is_err() {
        ok("&f64 rejects a duplicate NaN of a different payload");
    } else {
        bad("&f64 rejects a duplicate NaN of a different payload", "second insert succeeded".into());
    }
    db.create_uniq(Uniq { id: Uuid::nil(), serial: 0.0 }).expect("0.0");
    if db.create_uniq(Uniq { id: Uuid::nil(), serial: -0.0 }).is_err() {
        ok("&f64 treats -0.0 as a duplicate of 0.0");
    } else {
        bad("&f64 treats -0.0 as a duplicate of 0.0", "second insert succeeded".into());
    }

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} f64 index-key failure(s)");
        std::process::exit(1);
    }
    println!("all f64 index-key checks passed");
}
"##;
