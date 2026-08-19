//! `f64` index keys must be a **total order** (#242).
//!
//! # The defect this pins
//!
//! An `f64` index key was derived through `serde_json::Number::from_f64`, which
//! returns `None` for every non-finite value. That `None` fell through to the null
//! tag `\u{0}` — the same bucket as an unset optional or an unlinked FK. So NaN,
//! `+Inf` and `-Inf` were not merely lumped together, they were indistinguishable
//! from *absent*. And the ordered index (#169) excluded `f64` outright, because
//! `f64: !Ord` cannot key a `BTreeMap`, so `^f64` silently had no range method.
//!
//! Both are one problem — no total order over `f64` — so one encoding fixes both:
//!
//! ```text
//! let bits = v.to_bits();
//! let mask = ((bits as i64 >> 63) as u64) | 0x8000_0000_0000_0000;
//! bits ^ mask
//! ```
//!
//! For a non-negative float this flips only the sign bit, lifting positives into
//! the upper half of `u64`. For a negative one the arithmetic shift yields all-ones,
//! so every bit inverts — which both reverses the ordering within the negatives
//! (correct: IEEE magnitude runs backwards there) and drops them into the lower
//! half. The result orders `-Inf < negatives < ±0 < positives < +Inf < NaN` under
//! plain `u64: Ord`, and being a bijection on bit patterns it gives each non-finite
//! its own key for free.
//!
//! # Why "an index only narrows candidates" is not a defense
//!
//! It would be, if the generated probe re-checked the value. It does not: the live
//! `find_by_*` resolves the bucket and calls `get(id)` with no post-filter, the
//! snapshot `find_by_*_at` form recomputes the same colliding key, and `&unique`
//! enforcement rejects on bucket occupancy alone. A collision here is a wrong
//! answer, not a slow one — which is what the scenarios below assert.
//!
//! # Scope
//!
//! `f64?` still gets no *ordered* index: `ordered_key_type` returns `None` for any
//! nullable field regardless of inner type, which is orthogonal to this and
//! unchanged. What a nullable f64 gains here is the hash-side fix — `None` stops
//! colliding with NaN.
//!
//! Neither index is persisted (both rebuild in memory at open), so nothing here is
//! an on-disk format change.
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make f64-index-key      # or:
//! cargo test --test f64_index_key_test -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// Repo root — `CARGO_MANIFEST_DIR` is the crate this test compiles under.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Path dep line for a workspace substrate crate.
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

/// Every shape an `f64` key reaches: hash-indexed, nullable hash-indexed, unique,
/// and a composite component. `score` is the ordered-eligible one (non-nullable,
/// so it also gets the `BTreeMap`).
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
        // #333: `generate` claims this project id in the ledger under the
        // ForgeDB home. Without an override that is the developer's real
        // `~/.forgedb`, so two fixtures sharing a project name collide across
        // unrelated test runs — and the suite writes outside the tempdir.
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

/// Assert the probe returned exactly `want` — one row, the right one. "Exactly" is
/// the point: the defect returned a *superset*, so an `any(|r| r.id == want)` check
/// would have passed against the broken code.
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

/// A NaN that is not `f64::NAN`: different payload, sign bit set. Bit patterns
/// survive the fixed-width column verbatim, so this is what reaches the key.
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

    // One row per interesting point of the domain.
    let id_nan = db.create_sample(mk("a", f64::NAN, None)).expect("nan");
    let id_pinf = db.create_sample(mk("a", f64::INFINITY, None)).expect("+inf");
    let id_ninf = db.create_sample(mk("a", f64::NEG_INFINITY, None)).expect("-inf");
    let id_negzero = db.create_sample(mk("a", -0.0, None)).expect("-0.0");
    let id_one = db.create_sample(mk("a", 1.0, None)).expect("1.0");
    let id_negone = db.create_sample(mk("a", -1.0, None)).expect("-1.0");
    // The nullable side: a row whose `opt` is NaN, and one whose `opt` is unset.
    let id_optnan = db.create_sample(mk("b", 2.0, Some(f64::NAN))).expect("opt nan");
    let id_optnone = db.create_sample(mk("b", 3.0, None)).expect("opt none");
    // A NaN with a different payload, to prove payload folding.
    let id_othernan = db.create_sample(mk("c", other_nan(), None)).expect("other nan");

    // --- 1/2. NaN is not the null bucket -----------------------------------
    // `find_by_opt(None)` must mean "unset", not "unset or NaN".
    let none_rows = db.sample.find_by_opt(None);
    if none_rows.iter().any(|r| r.id == id_optnan) {
        bad("opt(None) excludes a NaN value", format!("got {:?}", ids_of(&none_rows)));
    } else if none_rows.iter().any(|r| r.id == id_optnone) {
        ok("opt(None) excludes a NaN value");
    } else {
        bad("opt(None) excludes a NaN value", "unset row missing entirely".into());
    }
    only("opt(NaN) finds only the NaN row", db.sample.find_by_opt(Some(f64::NAN)), id_optnan);

    // --- 3. the three non-finites are three distinct points -----------------
    // NaN returns BOTH NaN rows and nothing else — two rows, one key, by the
    // payload folding asserted below. What matters here is that it returns no
    // infinity and no unrelated row.
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

    // --- 4. -0.0 and 0.0 are the same key -----------------------------------
    // `0.0 == -0.0` is true in Rust, so a probe of `0.0` must find the row stored
    // as `-0.0`.
    only("score(0.0) finds the -0.0 row", db.sample.find_by_score(0.0), id_negzero);

    // --- 5. NaN payloads fold -----------------------------------------------
    // The row stored as `f64::NAN` and the one stored with a different payload
    // (and sign bit) are one bucket, so probing with EITHER NaN returns the same
    // pair. Probing with the other payload is what makes this a fold test rather
    // than a restatement of scenario 3.
    let mut nan_rows = ids_of(&db.sample.find_by_score(other_nan()));
    nan_rows.sort();
    if nan_rows == both_nans {
        ok("NaN payloads fold to one key");
    } else {
        bad("NaN payloads fold to one key", format!("expected {both_nans:?}, got {nan_rows:?}"));
    }

    // --- 6. the ordered index exists and orders correctly --------------------
    // Unbounded ascending: every row in true float order, non-finites at the ends.
    let asc = ids_of(&db.sample.find_by_score_range(None, None, false, None));
    let want_asc = vec![id_ninf, id_negone, id_negzero, id_one, id_pinf];
    // `bucket` b/c rows (2.0, 3.0, other-NaN) are in the map too; check the
    // relative order of the ones we pinned rather than the whole vector.
    let got: Vec<Uuid> = asc.iter().copied().filter(|i| want_asc.contains(i)).collect();
    if got == want_asc {
        ok("range(unbounded) orders -Inf < -1.0 < -0.0 < 1.0 < +Inf");
    } else {
        bad(
            "range(unbounded) orders -Inf < -1.0 < -0.0 < 1.0 < +Inf",
            format!("expected {want_asc:?}, got {got:?}"),
        );
    }
    // NaN sorts strictly above +Inf, so an unbounded ascending walk ends on it.
    if asc.last() == Some(&id_othernan) || asc.last() == Some(&id_nan) {
        ok("NaN sorts last");
    } else {
        bad("NaN sorts last", format!("range ended on {:?}", asc.last()));
    }

    // --- 7. a finite range admits neither NaN nor an infinity ----------------
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
    // Descending is the same set, reversed.
    let desc = ids_of(&db.sample.find_by_score_range(Some(0.0), Some(1.0), true, None));
    if desc == vec![id_one, id_negzero] {
        ok("range descending reverses");
    } else {
        bad("range descending reverses", format!("got {desc:?}"));
    }

    // --- 8. a composite index with a non-finite component --------------------
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

    // --- 9. `&f64` uniqueness is per-value, not per-null-bucket --------------
    // +Inf and -Inf are different values, so both must insert. Under the defect
    // they shared the null bucket and the second was rejected.
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
    // ...but a genuine duplicate is still rejected, including a NaN duplicate with
    // a different payload (it folds to the same key).
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
    // And -0.0 duplicates 0.0, since they compare equal.
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
