#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub const SUBSTRATE_PINS: &[&str] = &[
    "forgedb-storage",
    "forgedb-types",
    "forgedb-changefeed",
    "forgedb-wal",
    "forgedb-auth",
    "forgedb-query-params",
    "forgedb-compaction",
    "forgedb-txn",
    "forgedb-coordinator",
];

fn dep(name: &str) -> String {
    let crate_dir = name
        .strip_prefix("forgedb-")
        .unwrap_or_else(|| panic!("{name} is not a `forgedb-` substrate crate"));
    let path = repo_root().join("crates").join(crate_dir);
    assert!(
        path.join("Cargo.toml").is_file(),
        "SUBSTRATE_PINS names {name}, but {} has no manifest",
        path.display()
    );
    format!("{name} = {{ path = {:?} }}\n", path.to_string_lossy())
}

pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn cargo_toml(name: &str) -> String {
    let mut s = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n"
    );
    for n in SUBSTRATE_PINS {
        s.push_str(&dep(n));
    }
    s.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
    s.push_str("serde_json = \"1\"\n");
    s.push_str("regex = \"1\"\n");
    s.push_str("rust_decimal = { version = \"1\", features = [\"serde-with-str\"] }\n");
    s.push_str("utoipa = { version = \"5\", features = [\"uuid\"] }\n");
    s.push_str("utoipa-axum = \"0.2\"\n");
    s.push_str("axum = { version = \"0.8\", features = [\"ws\"] }\n");
    s.push_str("tokio = { version = \"1\", features = [\"full\"] }\n");
    s.push_str("tower = { version = \"0.5\", features = [\"util\"] }\n");
    s.push_str("tower-http = { version = \"0.6\", features = [\"trace\", \"cors\"] }\n");
    s.push_str("\n[workspace]\n");
    s
}

pub fn path_dep_cargo_toml(name: &str) -> String {
    cargo_toml(name)
}

pub fn generate_compile_run(tag: &str, schema: &str, driver: &str) -> (Output, PathBuf) {
    generate_compile_run_in(tag, schema, driver, None)
}

pub fn generate_compile_run_in(
    tag: &str,
    schema: &str,
    driver: &str,
    data_dir: Option<&Path>,
) -> (Output, PathBuf) {
    let proj = std::env::temp_dir().join(format!("forgedb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), schema);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let generated = Command::new(forgedb)
        .args(["generate", "all", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .env("FORGEDB_HOME", proj.join(".forgedb-home"))
        .output()
        .expect("run forgedb generate");
    assert!(
        generated.status.success(),
        "forgedb generate all failed:\n{}\n{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );

    write(&proj.join("Cargo.toml"), &cargo_toml(tag));
    write(&proj.join("src/main.rs"), driver);

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run cargo build");
    assert!(
        build.status.success(),
        "driver failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let out = Command::new(target.join(format!("debug/{tag}")))
        .arg(
            data_dir
                .map(Path::to_path_buf)
                .unwrap_or_else(|| proj.join("data")),
        )
        .arg(forgedb)
        .output()
        .expect("run driver");
    (out, proj)
}

pub fn build_warnings(proj: &Path) -> String {
    let out = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(proj)
        .env("CARGO_TARGET_DIR", proj.join("target"))
        .output()
        .expect("re-run cargo build to replay diagnostics");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn assert_driver_ok(out: &Output, proj: &Path, what: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(out.status.success(), "{what}:\n{stdout}\n{stderr}");
    let _ = std::fs::remove_dir_all(proj);
}

pub fn load_commands(bin: &Path) -> String {
    let tool = if cfg!(target_os = "macos") { "otool" } else { "ldd" };
    let mut cmd = Command::new(tool);
    if cfg!(target_os = "macos") {
        cmd.arg("-L");
    }
    let out = cmd.arg(bin).output().expect("inspect load commands");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn linked_libraries(bin: &Path) -> Vec<String> {
    parse_linked_libraries(&load_commands(bin))
}

pub fn parse_linked_libraries(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| !l.trim_end().ends_with(':'))
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let token = match line.split(" => ").nth(1) {
                Some(rhs) => rhs.split_whitespace().next()?,
                None => line.split_whitespace().next()?,
            };
            if token == "not" {
                return line.split_whitespace().next().map(str::to_string);
            }
            Some(token.to_string())
        })
        .collect()
}
