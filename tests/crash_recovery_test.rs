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

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make crash-test`)"]
fn committed_rows_survive_an_unclean_crash() {
    let proj = std::env::temp_dir().join(format!("forgedb-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(
        &proj.join("schema.forge"),
        "Account {\n  id: +uuid\n  balance: i64\n  owner: string\n}\n",
    );
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let gen_status = Command::new(forgedb)
        .args(["generate", "rust", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .env("FORGEDB_HOME", proj.join(".forgedb-home"))
        .status()
        .expect("run forgedb generate");
    assert!(gen_status.success(), "forgedb generate rust failed");

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

    let n = 50;
    let write_status = Command::new(&driver)
        .args(["write", &n.to_string(), data.to_string_lossy().as_ref()])
        .status()
        .expect("run driver write");
    assert!(
        !write_status.success(),
        "the write phase was supposed to abort (crash), but it exited cleanly"
    );

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

    let wal = data.join("account/wal.log");
    if wal.exists() {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&wal).unwrap();
        f.write_all(&[0xAB; 37]).unwrap();
        f.flush().unwrap();
        assert_eq!(
            count(&data),
            n,
            "a torn WAL tail corrupted the committed prefix (should be discarded)"
        );
    }

    let _ = std::fs::remove_dir_all(&proj);
}
