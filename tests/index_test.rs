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

const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::{Timestamp, Uuid};

static mut FAILURES: u32 = 0;

/// The `t_at` fixture value, in microseconds.
///
/// **DELIBERATELY NOT a whole number of milliseconds** (`…_890_123`), and that is the
/// point of it (#389).
///
/// This constant used to be millisecond-aligned, with a comment explaining that the
/// alignment was a workaround: `t_at` is `^timestamp`, quantum one millisecond, and the
/// write path floored the stored value while the index probe did not floor its argument,
/// so any value carrying a sub-millisecond remainder was filed under one key and looked
/// up under another. The workaround kept this guard green on exactly the broken path.
///
/// #389 floors both sides, so the remainder below is now carried through the write, the
/// hash-index key, the ordered-index bounds and the REST predicate identically. Removing
/// the workaround is the end-to-end guard: revert the fix and this test fails, which is
/// more than any assertion added alongside it would prove.
///
/// Do not "simplify" this back to a round number. A round number cannot fail.
const T_AT: i64 = 1_234_567_890_123;

/// What `T_AT` becomes once floored to `t_at`'s millisecond quantum — what is actually
/// stored, and what a reader gets back.
const T_AT_FLOORED: i64 = 1_234_567_890_000;

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
        t_at: Timestamp::from_micros(T_AT),
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
    // #389: probing with the UNFLOORED value must find the row. This is the assertion
    // the old millisecond-aligned fixture could not make, because with an aligned value
    // the floored and unfloored keys are the same string.
    hit("t_at", db.kitchen.find_by_t_at(Timestamp::from_micros(T_AT)), id_unset);
    // …and probing with the floored value must find it too: flooring is idempotent, so
    // both spellings of "the same instant" have to land in one bucket. If only one of
    // these two passes, the sides have been made consistent in the wrong direction.
    hit(
        "t_at (floored probe)",
        db.kitchen.find_by_t_at(Timestamp::from_micros(T_AT_FLOORED)),
        id_unset,
    );
    // #389, third site: the ORDERED index (`find_by_*_range`, #169) keys its bounds
    // through `ordered_key_expr`, the peer of the hash index's `index_value_expr`, and
    // it was not floored either. Only the `min` bound actually moves — stored keys are
    // all multiples of the quantum, so `Included(t)` and `Included(floor(t))` admit the
    // same multiples at the `max` end — but `Included(t)` EXCLUDES the bucket
    // `floor(t)` sits in, so the degenerate range over one instant returned nothing.
    //
    // Nothing in the tree exercised `_range` at all before this.
    hit(
        "t_at range [T_AT, T_AT]",
        db.kitchen.find_by_t_at_range(
            Some(Timestamp::from_micros(T_AT)),
            Some(Timestamp::from_micros(T_AT)),
            false,
            None,
        ),
        id_unset,
    );
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
    roundtrip(dir);
    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} round-trip failure(s)");
        std::process::exit(1);
    }
    println!("all index round-trip checks passed");
}
"##;
