//! Index-key parity: the monomorphic emission (#230) must produce **byte-identical**
//! keys to the `serde_json::Value` match it replaced, for every indexable type.
//!
//! # Why this test exists in this form
//!
//! The emitted key expression is inline inside private index paths, so nothing can
//! call it and diff it against the old form directly. (Making it callable would mean
//! emitting a per-field key function, which founders on nullable strings: the record
//! side holds `Option<String>` and the probe side `Option<&str>`, and no single
//! parameter type takes both without a per-call-site `as_deref()`. Inline emission
//! sidesteps that — binding through `&(...)` lets deref coercion serve both sides —
//! and that is exactly why it is not directly callable.)
//!
//! So the guard is a chain of two links:
//!
//!   1. **What the generator emits** is pinned at the string level by the snapshot
//!      and the per-type shape assertions in `crates/codegen/tests/codegen_snapshots.rs`.
//!   2. **That that form equals the legacy form** is pinned here: the driver holds
//!      the frozen pre-#230 generic implementation *and* a mirror of each emitted
//!      body, and asserts byte equality over an adversarial corpus.
//!
//! Link 2's mirror could drift from what is actually emitted; link 1 is what stops
//! that. If you change `RustGenerator::index_key_body`, both must move together.
//!
//! # The `bytes(64)` fields (#243)
//!
//! `s_hash` / `o_hash` are here to be *compiled*, not just keyed. serde implements
//! `Serialize`/`Deserialize` for `[T; N]` only up to N = 32, so before #243 a
//! `bytes(64)` field made the derive on the model struct fail to resolve and the
//! whole generated crate failed to build — indexed or not. This test compiles the
//! generated crate, so carrying an oversized field is what proves the ceiling is
//! gone; a string-level snapshot cannot.
//!
//! Their keys cannot be compared against `legacy` — that function *is* `to_value`,
//! and it does not accept `[u8; 64]`. That absence is the bug, not a gap in the
//! test. Instead the emitted form is anchored at N = 32, the last width serde can
//! serialize, and the widths past it are asserted to continue the same rendering.
//!
//! Plus an end-to-end pass: every indexed field is round-tripped through the **real**
//! generated `find_by_*`, which is the observable contract regardless of key bytes.
//!
//! # What byte-identity is and is not
//!
//! These keys are in-memory and rebuilt at open, so nothing on disk depends on them.
//! The actual invariants are that the record-side and probe-side keys agree, that
//! distinct values do not collide, and that the null bucket stays distinct from the
//! literal string `"null"` (#102). Byte-identity is held to as the cheapest sufficient
//! proxy for all three, not because the bytes themselves are a format.
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make index-key-parity      # or:
//! cargo test --test index_key_parity_test -- --ignored --nocapture
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

/// Every indexable type, in both its plain and nullable form, plus a required and an
/// optional FK — the full domain `indexed_fields` admits.
const SCHEMA: &str = r#"enum Status { Draft, Published, Archived }

Kitchen {
  id: +uuid
  s_name: &string
  s_code: ^bytes(8)
  s_hash: ^bytes(64)
  n_u32: ^u32
  n_u64: ^u64
  n_i32: ^i32
  n_i64: ^i64
  n_f64: ^f64
  b_flag: ^bool
  d_price: ^decimal
  u_ref: ^uuid
  t_at: ^timestamp
  e_status: ^Status

  o_name: ^string?
  o_code: ^bytes(8)?
  o_hash: ^bytes(64)?
  o_u32: ^u32?
  o_f64: ^f64?
  o_flag: ^bool?
  o_price: ^decimal?
  o_ref: ^uuid?
  o_at: ^timestamp?
  o_status: ^Status?

  owner: *Owner
  editor: ?Owner
}

Owner {
  id: +uuid
  email: &string
  kitchens: [Kitchen]
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make index-key-parity`)"]
fn monomorphic_index_keys_match_the_serde_json_form_byte_for_byte() {
    let proj = std::env::temp_dir().join(format!("forgedb-ikparity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), SCHEMA);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let gen_status = Command::new(forgedb)
        .args(["generate", "rust", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .status()
        .expect("run forgedb generate");
    assert!(gen_status.success(), "forgedb generate rust failed");

    let mut cargo_toml = String::from(
        "[package]\nname = \"ikdriver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
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

    let out = Command::new(target.join("debug/ikdriver"))
        .arg(proj.join("data"))
        .output()
        .expect("run driver");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(
        out.status.success(),
        "driver reported a parity failure:\n{stdout}\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&proj);
}

/// The driver. Two halves: `parity` (frozen legacy form vs a mirror of the emitted
/// form, byte for byte) and `roundtrip` (the real generated `find_by_*`).
const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::{Timestamp, Uuid};
use rust_decimal::Decimal;
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// FROZEN: the pre-#230 emission, verbatim. This is a historical reference, not
// live code — it must never be "improved" to track the generator, or the test
// stops comparing two independent things.
// ---------------------------------------------------------------------------
fn legacy<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::Null) => String::from('\u{0}'),
        Ok(serde_json::Value::String(s)) => {
            let mut k = String::from('\u{1}');
            k.push_str(&s);
            k
        }
        Ok(other) => {
            let mut k = String::from('\u{2}');
            k.push_str(&other.to_string());
            k
        }
        Err(_) => String::from('\u{3}'),
    }
}

// ---------------------------------------------------------------------------
// MIRROR of RustGenerator::index_key_body, one fn per arm. Keep in lockstep with
// crates/codegen/src/rust.rs; the shape assertions in codegen_snapshots.rs are
// what pin the generator side.
// ---------------------------------------------------------------------------
fn m_string(v: &str) -> String {
    let mut k = String::with_capacity(1 + v.len());
    k.push('\u{1}');
    k.push_str(v);
    k
}
fn m_uuid(v: &Uuid) -> String {
    let mut buf = [0u8; 36];
    let s: &str = v.hyphenated().encode_lower(&mut buf);
    let mut k = String::with_capacity(1 + s.len());
    k.push('\u{1}');
    k.push_str(s);
    k
}
fn m_decimal(v: &Decimal) -> String {
    let mut k = String::from('\u{1}');
    let _ = write!(k, "{}", v);
    k
}
fn m_status(v: &Status) -> String {
    // Mirrors the generated `__as_str` (private to the database module).
    let s = match v {
        Status::Draft => "Draft",
        Status::Published => "Published",
        Status::Archived => "Archived",
    };
    let mut k = String::with_capacity(1 + s.len());
    k.push('\u{1}');
    k.push_str(s);
    k
}
macro_rules! m_int {
    ($name:ident, $t:ty) => {
        fn $name(v: &$t) -> String {
            let mut k = String::from('\u{2}');
            let _ = write!(k, "{}", v);
            k
        }
    };
}
m_int!(m_u32, u32);
m_int!(m_u64, u64);
m_int!(m_i32, i32);
m_int!(m_i64, i64);
fn m_timestamp(v: &Timestamp) -> String {
    let mut k = String::from('\u{2}');
    let _ = write!(k, "{}", v.as_seconds());
    k
}
fn m_bool(v: &bool) -> String {
    let mut k = String::with_capacity(6);
    k.push('\u{2}');
    k.push_str(if *v { "true" } else { "false" });
    k
}
fn m_f64(v: &f64) -> String {
    match serde_json::Number::from_f64(*v) {
        Some(n) => {
            let mut k = String::from('\u{2}');
            let _ = write!(k, "{}", n);
            k
        }
        None => String::from('\u{0}'),
    }
}
fn m_bytes<const N: usize>(v: &[u8; N]) -> String {
    let mut k = String::from('\u{2}');
    k.push('[');
    for (i, b) in v.iter().enumerate() {
        if i > 0 {
            k.push(',');
        }
        let _ = write!(k, "{}", b);
    }
    k.push(']');
    k
}
fn m_opt<T>(v: &Option<T>, inner: impl Fn(&T) -> String) -> String {
    match v {
        Some(x) => inner(x),
        None => String::from('\u{0}'),
    }
}

static mut FAILURES: u32 = 0;

fn check(label: &str, mirrored: String, legacy_key: String) {
    if mirrored == legacy_key {
        println!("ok    {label:24} {:?}", mirrored);
    } else {
        println!("FAIL  {label:24} emitted={mirrored:?} legacy={legacy_key:?}");
        unsafe { FAILURES += 1 };
    }
}

fn parity() {
    // --- string class -----------------------------------------------------
    for s in ["", "hi", "null", "unicode \u{2713} \u{e9}", "\u{0}embedded"] {
        check("string", m_string(s), legacy(&s.to_string()));
    }
    for u in [Uuid::nil(), Uuid::new_v4()] {
        check("uuid", m_uuid(&u), legacy(&u));
    }
    // decimal: `index_value_expr` normalizes on BOTH sides before the key, so
    // parity is asserted on the key function alone. Scale invariance itself is
    // unchanged by #230 and is covered by the round-trip below.
    for d in ["0", "1", "1.0", "1.00", "-3.25", "79228162514264337593543950335"] {
        let d: Decimal = d.parse().unwrap();
        check("decimal", m_decimal(&d), legacy(&d));
        check("decimal-norm", m_decimal(&d.normalize()), legacy(&d.normalize()));
    }
    for st in [Status::Draft, Status::Published, Status::Archived] {
        check("enum", m_status(&st), legacy(&st));
    }

    // --- number / bool class ----------------------------------------------
    for v in [0u32, 1, u32::MAX] {
        check("u32", m_u32(&v), legacy(&v));
    }
    for v in [0u64, u64::MAX] {
        check("u64", m_u64(&v), legacy(&v));
    }
    for v in [i32::MIN, -1, 0, i32::MAX] {
        check("i32", m_i32(&v), legacy(&v));
    }
    for v in [i64::MIN, -1, 0, i64::MAX] {
        check("i64", m_i64(&v), legacy(&v));
    }
    for v in [true, false] {
        check("bool", m_bool(&v), legacy(&v));
    }
    for v in [i64::MIN, -1, 0, 1234567890, i64::MAX] {
        let t = Timestamp::from_seconds(v);
        check("timestamp", m_timestamp(&t), legacy(&t));
    }
    // The float arm is why `Display` cannot be used: 1.0 renders "1" but JSON
    // gives "1.0", and 1e300 renders "1e300" but JSON gives "1e+300".
    for v in [0.0f64, -0.0, 1.0, 1.5, -2.25, 1e300, 1e-300, f64::MIN, f64::MAX] {
        check("f64", m_f64(&v), legacy(&v));
    }
    // NaN / infinities: serde maps them to Value::Null, so they key into the
    // SAME bucket as None. Pre-existing behavior, deliberately preserved.
    for (name, v) in [("nan", f64::NAN), ("inf", f64::INFINITY), ("-inf", f64::NEG_INFINITY)] {
        check(&format!("f64-{name}"), m_f64(&v), legacy(&v));
        assert_eq!(m_f64(&v), "\u{0}", "{name} must land in the null bucket");
    }
    // bytes(N) is [u8; N] -> a JSON array, not a string.
    let code: [u8; 8] = *b"hello\0\0\0";
    check("bytes8", m_bytes(&code), legacy(&code));
    check("bytes8-zero", m_bytes(&[0u8; 8]), legacy(&[0u8; 8]));

    // --- past serde's array ceiling (#243) --------------------------------
    // N = 32 is the last width `legacy` can even be called at, so it is the anchor:
    // the emitted form agrees with the legacy form exactly there...
    let at32: [u8; 32] = [171; 32];
    check("bytes32-anchor", m_bytes(&at32), legacy(&at32));
    // ...and every width past it continues the same rendering, element by element.
    // This is what makes the generated `__forgedb_big_bytes` wire-continuous with
    // serde's own impl instead of merely plausible.
    let at33: [u8; 33] = [171; 33];
    let grown = format!("{},171]", m_bytes(&at32).trim_end_matches(']'));
    check("bytes33-continues-32", m_bytes(&at33), grown);
    let at64: [u8; 64] = [171; 64];
    check("bytes64-elements", format!("{}", m_bytes(&at64).matches(',').count()), "63".to_string());
    check("opt-bytes64", m_opt(&Some(at64), m_bytes), m_bytes(&at64));
    check("opt-bytes64-none", m_opt(&None::<[u8; 64]>, m_bytes), "\u{0}".to_string());

    // --- nullable ---------------------------------------------------------
    check("opt-none", m_opt(&Option::<String>::None, |s: &String| m_string(s)), legacy(&Option::<String>::None));
    // The #102 collision: None must not key like the literal string "null".
    let some_null = Some("null".to_string());
    check("opt-some-null", m_opt(&some_null, |s: &String| m_string(s)), legacy(&some_null));
    assert_ne!(
        m_opt(&some_null, |s: &String| m_string(s)),
        m_opt(&Option::<String>::None, |s: &String| m_string(s)),
        "Some(\"null\") must not collide with None"
    );
    // And Some("") must not collide with None either.
    let some_empty = Some(String::new());
    check("opt-some-empty", m_opt(&some_empty, |s: &String| m_string(s)), legacy(&some_empty));
    assert_ne!(
        m_opt(&some_empty, |s: &String| m_string(s)),
        m_opt(&Option::<String>::None, |s: &String| m_string(s)),
        "Some(\"\") must not collide with None"
    );
    let ou = Some(Uuid::new_v4());
    check("opt-uuid", m_opt(&ou, m_uuid), legacy(&ou));
    check("opt-uuid-none", m_opt(&None::<Uuid>, m_uuid), legacy(&None::<Uuid>));
    let of = Some(1.0f64);
    check("opt-f64", m_opt(&of, m_f64), legacy(&of));
    let ost = Some(Status::Published);
    check("opt-enum", m_opt(&ost, m_status), legacy(&ost));
    let oc = Some(code);
    check("opt-bytes8", m_opt(&oc, m_bytes), legacy(&oc));
}

/// The observable contract, through the real generated index paths.
fn roundtrip(dir: std::path::PathBuf) {
    let mut db = Database::open_at(dir);
    let owner = db
        .create_owner(Owner { id: Uuid::nil(), email: "o@x.test".into(), kitchens: () })
        .expect("owner");
    let uref = Uuid::new_v4();

    let base = |name: &str| Kitchen {
        id: Uuid::nil(),
        s_name: name.to_string(),
        s_code: *b"hello\0\0\0",
        s_hash: [171u8; 64],
        n_u32: 7,
        n_u64: u64::MAX,
        n_i32: -5,
        n_i64: -9,
        n_f64: 1.0,
        b_flag: true,
        d_price: "1.00".parse().unwrap(),
        u_ref: uref,
        t_at: Timestamp::from_seconds(1234567890),
        e_status: Status::Published,
        o_name: None,
        o_code: None,
        o_hash: None,
        o_u32: None,
        o_f64: None,
        o_flag: None,
        o_price: None,
        o_ref: None,
        o_at: None,
        o_status: None,
        owner,
        editor: None,
    };

    let mut unset = base("unset");
    unset.o_name = None;
    let id_unset = db.create_kitchen(unset).expect("insert unset");

    let mut literal_null = base("literal-null");
    literal_null.o_name = Some("null".to_string());
    let id_null = db.create_kitchen(literal_null).expect("insert literal-null");

    let mut empty = base("empty");
    empty.o_name = Some(String::new());
    let id_empty = db.create_kitchen(empty).expect("insert empty");

    // Every plain indexed field resolves through its own index.
    let hit = |label: &str, rows: Vec<Kitchen>, want: Uuid| {
        if rows.iter().any(|r| r.id == want) {
            println!("ok    find_by_{label}");
        } else {
            println!("FAIL  find_by_{label} did not return the inserted row");
            unsafe { FAILURES += 1 };
        }
    };
    hit("s_name", db.kitchen.find_by_s_name("unset"), id_unset);
    hit("s_code", db.kitchen.find_by_s_code(*b"hello\0\0\0"), id_unset);
    // #243: the same path at a width serde cannot derive for. Reaching this line at
    // all means the generated crate compiled with an oversized `bytes(N)` field.
    hit("s_hash", db.kitchen.find_by_s_hash([171u8; 64]), id_unset);
    hit("o_hash(None)", db.kitchen.find_by_o_hash(None), id_unset);
    hit("n_u32", db.kitchen.find_by_n_u32(7), id_unset);
    hit("n_u64", db.kitchen.find_by_n_u64(u64::MAX), id_unset);
    hit("n_i32", db.kitchen.find_by_n_i32(-5), id_unset);
    hit("n_i64", db.kitchen.find_by_n_i64(-9), id_unset);
    hit("n_f64", db.kitchen.find_by_n_f64(1.0), id_unset);
    hit("b_flag", db.kitchen.find_by_b_flag(true), id_unset);
    hit("u_ref", db.kitchen.find_by_u_ref(uref), id_unset);
    hit("t_at", db.kitchen.find_by_t_at(Timestamp::from_seconds(1234567890)), id_unset);
    hit("e_status", db.kitchen.find_by_e_status(Status::Published), id_unset);
    hit("owner", db.kitchen.find_by_owner(owner), id_unset);

    // decimal stays scale-invariant: stored "1.00", probed "1.0".
    hit("d_price", db.kitchen.find_by_d_price("1.0".parse().unwrap()), id_unset);

    // The #102 buckets are three distinct ones.
    hit("o_name(None)", db.kitchen.find_by_o_name(None), id_unset);
    hit("o_name(\"null\")", db.kitchen.find_by_o_name(Some("null")), id_null);
    hit("o_name(\"\")", db.kitchen.find_by_o_name(Some("")), id_empty);
    for (label, rows, forbidden) in [
        ("o_name(None) excludes \"null\"", db.kitchen.find_by_o_name(None), id_null),
        ("o_name(\"null\") excludes None", db.kitchen.find_by_o_name(Some("null")), id_unset),
        ("o_name(\"\") excludes None", db.kitchen.find_by_o_name(Some("")), id_unset),
    ] {
        if rows.iter().any(|r| r.id == forbidden) {
            println!("FAIL  {label}");
            unsafe { FAILURES += 1 };
        } else {
            println!("ok    {label}");
        }
    }

    // The optional FK: unlinked rows key into the null bucket, not a uuid one.
    hit("editor(None)", db.kitchen.find_by_editor(None), id_unset);
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir"));
    parity();
    roundtrip(dir);
    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} parity/round-trip failure(s)");
        std::process::exit(1);
    }
    println!("all index-key parity checks passed");
}
"##;
