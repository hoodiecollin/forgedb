//! End-to-end crash-recovery test (#16 — substantiates `docs/.../durability`).
//!
//! Durability claims are only credible if committed data survives an *unclean*
//! termination — not a graceful shutdown that flushes on the way out. This test
//! proves that against the **real generated code**, end to end:
//!
//!   1. Generate `database.rs` for a small schema with the `forgedb` CLI.
//!   2. Compile a tiny driver binary around it.
//!   3. Run the driver to insert N rows, then `std::process::abort()` — SIGABRT,
//!      no destructors, no `Drop` flush, no clean close. The only thing that can
//!      have made the rows durable is the per-insert WAL commit + fsync.
//!   4. Reopen in a fresh process and assert all N rows are present.
//!   5. Corrupt the tail of the WAL (a torn next-write) and reopen again: the
//!      committed prefix must still be intact and the open must not panic.
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make crash-test            # or:
//! cargo test --test crash_recovery_test -- --ignored --nocapture
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

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make crash-test`)"]
fn committed_rows_survive_an_unclean_crash() {
    let proj = std::env::temp_dir().join(format!("forgedb-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    // 1. Schema + generated database.rs.
    write(
        &proj.join("schema.forge"),
        "Account {\n  id: +uuid\n  balance: i64\n  owner: string\n}\n",
    );
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

    // 2. A driver crate around the generated module.
    let mut cargo_toml = String::from(
        "[package]\nname = \"crashdriver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
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

    // The driver: `write N <dir>` inserts N accounts then aborts (unclean);
    // `count <dir>` reopens and prints the live row count.
    write(
        &proj.join("src/main.rs"),
        r#"mod database;
use database::*;
use forgedb_types::Uuid;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "write" => {
            let n: usize = args[2].parse().unwrap();
            let dir = std::path::PathBuf::from(&args[3]);
            let mut db = Database::open_at(dir);
            for i in 0..n {
                db.create_account(Account {
                    id: Uuid::nil(),
                    balance: i as i64,
                    owner: format!("owner{i}"),
                })
                .expect("insert");
            }
            // Crash: no clean shutdown, no Drop, no flush-on-exit. If the rows
            // survive, per-insert WAL commit made them durable — not a graceful close.
            std::process::abort();
        }
        "count" => {
            let dir = std::path::PathBuf::from(&args[2]);
            let db = Database::open_at(dir);
            println!("{}", db.account.all().len());
        }
        other => panic!("bad cmd: {other}"),
    }
}
"#,
    );

    // 3. Compile the driver (own target dir so the outer test's env can't redirect it).
    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("run cargo build");
    assert!(build.success(), "driver failed to compile");
    let driver = target.join("debug/crashdriver");

    let data = proj.join("data");

    // 4. Insert 50 rows then abort. The process must NOT exit cleanly.
    let n = 50;
    let write_status = Command::new(&driver)
        .args(["write", &n.to_string(), data.to_string_lossy().as_ref()])
        .status()
        .expect("run driver write");
    assert!(
        !write_status.success(),
        "the write phase was supposed to abort (crash), but it exited cleanly"
    );

    // 5. Reopen in a fresh process: every committed row must be there.
    let count = |data: &Path| -> usize {
        let out = Command::new(&driver)
            .args(["count", data.to_string_lossy().as_ref()])
            .output()
            .expect("run driver count");
        assert!(out.status.success(), "count phase failed to open the crashed data dir");
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    };
    assert_eq!(
        count(&data),
        n,
        "committed rows were lost across an unclean crash — durability is not honored"
    );

    // 6. Torn WAL tail: append a partial next-write's worth of garbage to the
    //    account WAL, then reopen. The CRC-framed replay must discard the torn
    //    tail; the committed prefix must remain intact and the open must not panic.
    let wal = data.join("account/wal.log");
    if wal.exists() {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&wal).unwrap();
        f.write_all(&[0xAB; 37]).unwrap(); // a torn frame: bytes with no valid CRC
        f.flush().unwrap();
        assert_eq!(
            count(&data),
            n,
            "a torn WAL tail corrupted the committed prefix (should be discarded)"
        );
    }

    let _ = std::fs::remove_dir_all(&proj);
}
