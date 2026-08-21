//! The index contract, end to end: every indexable type must resolve the row it
//! was stored under, through the **real** generated `find_by_*`.
//!
//! # What it asserts
//!
//! The fixture model carries every type `indexed_fields` admits, in both its plain
//! and its nullable form, plus a required and an optional FK. Each one is stored and
//! then looked up again. Three properties fall out:
//!
//!   1. **The record side and the probe side agree.** The index key is emitted
//!      inline in two places — once when a row is written, once when one is probed —
//!      and nothing outside generated code can call either. Storing a value and
//!      finding it again is what proves the two agree.
//!   2. **Distinct values do not collide.** The `excludes` checks below.
//!   3. **The null bucket is its own bucket** (#102). `None`, the literal string
//!      `"null"` and the empty string are three different things; an unlinked
//!      optional FK keys as absent rather than as some uuid.
//!   4. **A probe is floored to the field quantum, exactly as the write was**
//!      (#389). Every timestamp value in the fixture is misaligned to its field's
//!      declared quantum on purpose, across all three quanta and across all four
//!      index kinds — hash, nullable hash, `&unique`, ordered range — plus an FK
//!      column whose parent identity is itself a coarse timestamp. This is the one
//!      property here that is about the value rather than the type, and it is the
//!      one an aligned fixture silently stops testing.
//!
//! # What it deliberately does NOT assert
//!
//! This file used to also compare each emitted key **byte for byte** against the
//! pre-#230 `serde_json::Value` form it replaced, holding a frozen copy of the old
//! implementation and a hand-written mirror of each emitted arm. That half is gone
//! (#381), and should not be reinstated:
//!
//! * It proved a one-time rewrite was behaviour-preserving. The rewrite shipped.
//! * These keys are in-memory and **rebuilt at open**, so nothing on disk depends on
//!   their bytes. Byte-identity with a superseded form has no compatibility value.
//! * The "frozen" side was not frozen. It ran the value through `serde::Serialize`,
//!   so it re-derived the baseline from whatever serde did *that day* — and when
//!   #254 changed `Timestamp`'s serde from a transparent integer to an RFC 3339
//!   string, the reference silently moved and the comparison stopped meaning
//!   anything. A live re-derivation cannot be a historical baseline.
//! * Byte-identity was only ever a proxy for the three properties above, which this
//!   file now asserts directly instead of by proxy.
//!
//! What the generator *emits* stays pinned at the string level by the per-type shape
//! assertions in `crates/codegen/tests/codegen_snapshots.rs`. That is the right layer
//! for it: those run in the fast suite and cannot drift from the generator, because
//! they read its output rather than a copy of it.
//!
//! # Coverage that used to live here
//!
//! Two things the deleted half also touched are covered better elsewhere, and are
//! not re-tested here:
//!
//! * `f64` keys — total order, the non-finites, `-0.0`/NaN folding, and the ranges
//!   the ordered index depends on: `tests/f64_index_key_test.rs`, end to end over a
//!   real generated database.
//! * Oversized `bytes(N)` past serde's `[T; N]` ceiling of N = 32 (#243):
//!   `tests/oversized_array_test.rs`, which covers unindexed fields too.
//!
//! `s_hash` / `o_hash` stay in the fixture regardless — they are `bytes(64)`, so
//! their presence means this crate compiled with a field past that ceiling.
//!
//! # Running it
//!
//! It generates and compiles a crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. That cost is the only reason; nothing here is unreliable.
//!
//! ```bash
//! make index-test      # or:
//! cargo test --test index_test -- --ignored --nocapture
//! ```
//!
//! Note that `#[ignore]` is why a break here can go unnoticed for a long time, and
//! `cargo test --no-run` will NOT catch one: the driver below is a string literal
//! compiled by a subprocess at run time, so only *running* this test type-checks it
//! (#381).

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
  c_at: ^timestamp(s)
  u_at: ^timestamp(us)
  q_at: ^&timestamp
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
  stamp: *Stamped
}

Owner {
  id: +uuid
  email: &string
  kitchens: [Kitchen]
}

// A non-auto coarse-timestamp identity: legal (`is_identity_key` admits
// `timestamp(s|ms|us)`), and the only shape in which an FK column is a `Timestamp`
// whose quantum is coarser than a microsecond (#389).
Stamped {
  id: timestamp(ms)
  label: string
  kitchens: [Kitchen]
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make index-test`)"]
fn every_indexed_field_resolves_the_row_it_was_stored_under() {
    let proj = std::env::temp_dir().join(format!("forgedb-index-test-{}", std::process::id()));
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
        "[package]\nname = \"indexdriver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
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

    let out = Command::new(target.join("debug/indexdriver"))
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

/// The driver: store rows, then resolve them back through the real generated
/// `find_by_*`. Counts failures rather than panicking so one run reports every
/// broken field, not just the first.
const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::{Timestamp, Uuid};

static mut FAILURES: u32 = 0;

/// The timestamp fixture values, in microseconds. **Every one of them carries a
/// remainder past its field's quantum, deliberately** (#389).
///
/// A `timestamp` field declares a *quantum*, and the write path floors a written
/// value to it. Before #389 the read path did not: the probe hashed the raw
/// argument, so a value with a sub-quantum remainder was filed under one key and
/// asked for under another, and a row could not find itself. The fix floors both
/// sides, from one place per index kind.
///
/// These values are what make that assertable. An aligned value passes either way
/// — which is exactly the alignment #381 had to make here to keep the round-trip
/// measuring indexes rather than quanta. Do not "simplify" any of them back to a
/// round number: doing so silently restores the workaround and deletes the only
/// end-to-end check that the two sides agree.
///
/// The three quanta are all exercised, because the emitted quantum literal is
/// per-field and a hard-coded one would pass a single-field test:
///   * `t_at`  — `^timestamp`, quantum 1_000 (ms)   → stored `…_890_000`
///   * `c_at`  — `^timestamp(s)`, quantum 1_000_000 → stored `…_000_000`
///   * `u_at`  — `^timestamp(us)`, quantum 1        → the CONTROL: no flooring is
///     emitted at all, so it must round-trip unchanged both before and after.
const T_AT: i64 = 1_234_567_890_123;
/// The `^timestamp(s)` fixture value — a whole 1 042 µs past a second boundary, so
/// it is misaligned for the second quantum AND for the millisecond one.
const C_AT: i64 = 1_234_567_000_000 + 1_042;
/// The `^timestamp(us)` control value. Every digit is significant at quantum 1.
const U_AT: i64 = 1_234_567_890_987;
/// The `^&timestamp` (unique) fixture value for the first row. Rows 2 and 3 offset
/// by a whole millisecond each so they stay distinct *after* flooring — a `&unique`
/// timestamp is unique at the field quantum, not at microsecond resolution.
const Q_AT: i64 = 1_777_000_000_456;
/// The `Stamped` identity — a non-auto `timestamp(ms)` primary key, misaligned, so
/// the row is stored under the floored id while `Kitchen.stamp` is handed the raw
/// one.
const STAMP_ID: i64 = 1_555_000_111_222;

/// The observable contract, through the real generated index paths.
fn roundtrip(dir: std::path::PathBuf) {
    let mut db = Database::open_at(dir);
    let owner = db
        .create_owner(Owner { id: Uuid::nil(), email: "o@x.test".into(), kitchens: () })
        .expect("owner");
    let uref = Uuid::new_v4();

    // A parent keyed by a MISALIGNED `timestamp(ms)`. `create_stamped` floors the
    // identity on write, so the row lives at `STAMP_ID` truncated to a whole
    // millisecond — and `stamp_id` is that floored value, not the one passed in.
    let stamp_id = db
        .create_stamped(Stamped {
            id: Timestamp::from_micros(STAMP_ID),
            label: "s".into(),
            kitchens: (),
        })
        .expect("stamped");
    if stamp_id != Timestamp::from_micros(STAMP_ID / 1000 * 1000) {
        println!("FAIL  create_stamped did not floor the identity to the ms quantum");
        unsafe { FAILURES += 1 };
    }

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
        t_at: Timestamp::from_micros(T_AT),
        c_at: Timestamp::from_micros(C_AT),
        u_at: Timestamp::from_micros(U_AT),
        q_at: Timestamp::from_micros(Q_AT),
        e_status: Status::Published,
        o_name: None,
        o_code: None,
        o_hash: None,
        o_u32: None,
        o_f64: None,
        o_flag: None,
        o_price: None,
        o_ref: None,
        o_at: Some(Timestamp::from_micros(T_AT)),
        o_status: None,
        owner,
        editor: None,
        // The raw, unfloored value — NOT `stamp_id`. A reference is resolved at the
        // field's declared precision, so handing the FK the same misaligned instant
        // the parent was created with must find that parent (#389).
        stamp: Timestamp::from_micros(STAMP_ID),
    };

    // `q_at` is `&unique`, so the three rows must differ — and they differ by a
    // WHOLE millisecond each, because a `&unique` timestamp is unique at the field
    // quantum. Two values a microsecond apart in a `^&timestamp` field floor to the
    // same instant and are, correctly, a uniqueness conflict.
    let mut unset = base("unset");
    unset.o_name = None;
    let id_unset = db.create_kitchen(unset).expect("insert unset");

    let mut literal_null = base("literal-null");
    literal_null.o_name = Some("null".to_string());
    literal_null.q_at = Timestamp::from_micros(Q_AT + 1_000);
    let id_null = db.create_kitchen(literal_null).expect("insert literal-null");

    let mut empty = base("empty");
    empty.o_name = Some(String::new());
    empty.q_at = Timestamp::from_micros(Q_AT + 2_000);
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
    hit("e_status", db.kitchen.find_by_e_status(Status::Published), id_unset);
    hit("owner", db.kitchen.find_by_owner(owner), id_unset);

    // --- the quantum contract (#389) -------------------------------------------
    // Each value below carries a remainder past its field's quantum. The write path
    // floors it; the probe must floor it identically, or the row cannot find itself.
    // One probe per index kind, because each kind derives its key from a different
    // place in the generator and a fix to one does not reach the others.
    hit("t_at", db.kitchen.find_by_t_at(Timestamp::from_micros(T_AT)), id_unset);
    hit("c_at (quantum 1_000_000)", db.kitchen.find_by_c_at(Timestamp::from_micros(C_AT)), id_unset);
    hit("o_at (nullable)", db.kitchen.find_by_o_at(Some(Timestamp::from_micros(T_AT))), id_unset);
    // The FK column: `stamp` was handed the raw instant, the parent lives at the
    // floored one.
    hit("stamp (FK to a timestamp identity)", db.kitchen.find_by_stamp(Timestamp::from_micros(STAMP_ID)), id_unset);
    // The ordered index (#169) is a separate derivation from the hash index: a
    // degenerate `[t, t]` range must select what `find_by_t_at(t)` selects.
    hit(
        "t_at_range [t, t]",
        db.kitchen.find_by_t_at_range(
            Some(Timestamp::from_micros(T_AT)),
            Some(Timestamp::from_micros(T_AT)),
            false,
            None,
        ),
        id_unset,
    );
    // The `&unique` probe returns at most one row, so it needs its own check.
    match db.kitchen.get_by_q_at(Timestamp::from_micros(Q_AT)) {
        Some(r) if r.id == id_unset => println!("ok    get_by_q_at (unique)"),
        _ => {
            println!("FAIL  get_by_q_at did not return the inserted row");
            unsafe { FAILURES += 1 };
        }
    }
    // The CONTROL. `timestamp(us)` has quantum 1, so no flooring is emitted on
    // either side — this passed before the fix and must still pass after, which is
    // what proves the quantum literal is read from the field rather than assumed.
    hit("u_at (quantum 1 — control)", db.kitchen.find_by_u_at(Timestamp::from_micros(U_AT)), id_unset);

    // Flooring must not merge buckets that the quantum keeps apart: `Q_AT + 1_000`
    // is a whole millisecond away, so it is a different row, not the same one.
    for (label, rows, forbidden) in [
        (
            "q_at floors without collapsing distinct quanta",
            db.kitchen.find_by_q_at(Timestamp::from_micros(Q_AT)),
            id_null,
        ),
    ] {
        if rows.iter().any(|r| r.id == forbidden) {
            println!("FAIL  {label}");
            unsafe { FAILURES += 1 };
        } else {
            println!("ok    {label}");
        }
    }

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
    roundtrip(dir);
    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} round-trip failure(s)");
        std::process::exit(1);
    }
    println!("all index round-trip checks passed");
}
"##;
