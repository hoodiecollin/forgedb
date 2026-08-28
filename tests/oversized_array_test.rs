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

const SCHEMA: &str = r#"struct Fp {
  digest: bytes(64)
  small: bytes(8)
  wide: [u32; 40]
}

Doc {
  id: +uuid
  plain: bytes(64)
  fingerprint: ^bytes(64)
  opt_hash: bytes(48)?
  boundary: bytes(32)
  past: bytes(33)
  small: ^bytes(8)
  fp: Fp
  arr_big: [bytes(64); 2]
  arr_small: [bytes(8); 2]
  many: [u32; 40]
  few: [u32; 4]
  many_uuid: [uuid; 33]
  name: string
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make oversized-array-test`)"]
fn oversized_arrays_compile_and_round_trip() {
    let proj = std::env::temp_dir().join(format!("forgedb-bigarray-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), SCHEMA);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let gen_status = Command::new(forgedb)
        .args([
            "generate", "rust", "--output", "src", "--schema", "schema.forge",
        ])
        .current_dir(&proj)
        .env("FORGEDB_HOME", proj.join(".forgedb-home"))
        .status()
        .expect("run forgedb generate");
    assert!(gen_status.success(), "forgedb generate rust failed");

    let mut cargo_toml = String::from(
        "[package]\nname = \"bigarray\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
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
    assert!(
        build.success(),
        "the generated crate must compile with oversized array fields"
    );

    let out = Command::new(target.join("debug/bigarray"))
        .output()
        .expect("run driver");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(
        out.status.success(),
        "driver reported a round-trip failure:\n{stdout}\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&proj);
}

const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::Uuid;

fn sample() -> Doc {
    Doc {
        id: Uuid::nil(),
        plain: [7u8; 64],
        fingerprint: [9u8; 64],
        opt_hash: Some([3u8; 48]),
        boundary: [1u8; 32],
        past: [2u8; 33],
        small: *b"hello\0\0\0",
        fp: Fp { digest: [4u8; 64], small: [5u8; 8], wide: [6u32; 40] },
        arr_big: [[8u8; 64], [9u8; 64]],
        arr_small: [[1u8; 8], [2u8; 8]],
        many: [11u32; 40],
        few: [12u32; 4],
        many_uuid: [Uuid::nil(); 33],
        name: "n".to_string(),
    }
}

static mut FAILURES: u32 = 0;

fn check(label: &str, ok: bool) {
    if ok {
        println!("ok    {label}");
    } else {
        println!("FAIL  {label}");
        unsafe { FAILURES += 1 };
    }
}

fn main() {
    let d = sample();
    let js = serde_json::to_value(&d).expect("serialize");

    // --- shape: continuous across the ceiling ------------------------------
    // A field's wire form must not change at N = 32. `boundary`/`few`/`small` use
    // serde's own impl and `past`/`many`/`plain` use the generated one; if the two
    // disagreed, the boundary is where a client would see it.
    for (key, len) in [
        ("plain", 64usize), ("past", 33), ("boundary", 32),
        ("many", 40), ("few", 4), ("many_uuid", 33),
    ] {
        let got = js[key].as_array().map(|a| a.len());
        check(&format!("{key} is a JSON array of {len}"), got == Some(len));
    }
    check("arr_big is 2 arrays of 64", js["arr_big"].as_array().map(|a| a.len()) == Some(2)
        && js["arr_big"][0].as_array().map(|a| a.len()) == Some(64));
    check("arr_small is 2 arrays of 8", js["arr_small"][0].as_array().map(|a| a.len()) == Some(8));
    // The inline struct: a second emission site that broke independently.
    check("struct field digest is 64", js["fp"]["digest"].as_array().map(|a| a.len()) == Some(64));
    check("struct field wide is 40", js["fp"]["wide"].as_array().map(|a| a.len()) == Some(40));

    // --- round-trip --------------------------------------------------------
    let back: Doc = serde_json::from_value(js.clone()).expect("deserialize");
    check("plain round-trips", back.plain == d.plain);
    check("past round-trips", back.past == d.past);
    check("opt_hash round-trips", back.opt_hash == d.opt_hash);
    check("struct digest round-trips", back.fp.digest == d.fp.digest);
    check("struct wide round-trips", back.fp.wide == d.fp.wide);
    check("arr_big round-trips", back.arr_big == d.arr_big);
    check("many round-trips", back.many == d.many);
    check("many_uuid round-trips", back.many_uuid == d.many_uuid);

    // --- nullable ----------------------------------------------------------
    let mut null_js = js.clone();
    null_js["opt_hash"] = serde_json::Value::Null;
    let n: Doc = serde_json::from_value(null_js).expect("deserialize null");
    check("an oversized nullable reads null as None", n.opt_hash.is_none());

    // --- length rejection --------------------------------------------------
    for (key, bad) in [
        ("plain", serde_json::json!([1, 2, 3])),
        ("many", serde_json::json!([1, 2, 3])),
        ("arr_big", serde_json::json!([[1, 2, 3], [1, 2, 3]])),
    ] {
        let mut j = js.clone();
        j[key] = bad;
        check(
            &format!("{key} rejects a short array"),
            serde_json::from_value::<Doc>(j).is_err(),
        );
    }
    let mut long = js.clone();
    let mut v = long["many"].as_array().unwrap().clone();
    v.push(serde_json::json!(99));
    long["many"] = serde_json::Value::Array(v);
    check("many rejects an over-long array", serde_json::from_value::<Doc>(long).is_err());

    let mut long_bytes = js.clone();
    let mut b = long_bytes["plain"].as_array().unwrap().clone();
    b.push(serde_json::json!(1));
    long_bytes["plain"] = serde_json::Value::Array(b);
    check("plain rejects an over-long array", serde_json::from_value::<Doc>(long_bytes).is_err());

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} failure(s)");
        std::process::exit(1);
    }
    println!("all oversized-array checks passed");
}
"##;
