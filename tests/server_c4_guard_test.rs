use forgedb_codegen::ServerPackage;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const REGION_START: &str = "    let tenant = std::env::var(\"FORGEDB_TENANT\").ok();";
const REGION_END: &str = "    let db = std::sync::Arc::new(";

fn lift_fn(source: &str, name: &str) -> String {
    let head = format!("fn {name}");
    let start = source
        .find(&head)
        .unwrap_or_else(|| panic!("the generated server no longer defines `{head}`"));
    let rest = &source[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`{head}` has no top-level closing brace"));
    rest[..end + 3].to_string()
}

fn guard_program() -> String {
    let main_rs = ServerPackage::main_rs();

    let start = main_rs.find(REGION_START).unwrap_or_else(|| {
        panic!("the generated server no longer starts its data-dir resolution with:\n{REGION_START}")
    });
    let end = main_rs[start..]
        .find(REGION_END)
        .map(|e| start + e)
        .unwrap_or_else(|| {
            panic!("the generated server no longer opens the database with:\n{REGION_END}")
        });
    let region = &main_rs[start..end];

    assert!(
        region.contains("refusing to open a database inside the ForgeDB build cache"),
        "the extracted region does not contain the C4 refusal — the anchors have \
         drifted and this test is compiling something else:\n{region}"
    );
    assert!(
        region.contains("std::process::exit(1)"),
        "the extracted region no longer exits on refusal:\n{region}"
    );

    format!(
        "#![allow(unused)]\n\
         fn main() {{\n{region}\n    \
         println!(\"OPENED\");\n}}\n\n{}\n\n{}\n",
        lift_fn(&main_rs, "forgedb_home_dir()"),
        lift_fn(&main_rs, "closest_real_ancestor("),
    )
}

fn compile(dir: &Path) -> PathBuf {
    let src = dir.join("guard.rs");
    std::fs::write(&src, guard_program()).expect("write guard.rs");
    let bin = dir.join("guard");
    let out = Command::new("rustc")
        .args(["--edition", "2021", "-O", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("run rustc");
    assert!(
        out.status.success(),
        "the extracted C4 guard does not compile:\n{}\n\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        std::fs::read_to_string(&src).unwrap_or_default()
    );
    bin
}

fn run(bin: &Path, cwd: &Path, home: &Path, data: &str) -> (bool, String) {
    let out = Command::new(bin)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .env("FORGEDB_DATA", data)
        .env_remove("FORGEDB_TENANT")
        .output()
        .expect("run the guard");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

#[test]
fn scenario_36_the_server_refuses_a_database_inside_the_forgedb_home() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    let project = home.join("projects").join("app");
    std::fs::create_dir_all(&project).unwrap();

    let (ok, log) = run(&bin, &project, &home, "data");
    assert!(!ok, "the server opened a database inside the cache:\n{log}");
    assert!(
        log.contains("refusing to open a database inside the ForgeDB build cache"),
        "the refusal does not name the build cache:\n{log}"
    );
    assert!(
        log.contains("FORGEDB_DATA"),
        "the refusal does not tell the user what to pass instead:\n{log}"
    );
    assert!(
        log.contains(&home.display().to_string())
            || log.contains(&std::fs::canonicalize(&home).unwrap().display().to_string()),
        "the refusal does not print the home it compared against:\n{log}"
    );
    assert!(!log.contains("OPENED"), "it refused AND opened:\n{log}");
}

#[test]
fn scenario_36_a_data_root_outside_the_home_is_opened() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    let elsewhere = tmp.path().join("my-project");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let (ok, log) = run(&bin, &elsewhere, &home, "data");
    assert!(ok, "a data root outside the home was refused:\n{log}");
    assert!(log.contains("OPENED"), "the guard did not fall through:\n{log}");
}

#[test]
fn scenario_36_the_remedy_the_message_names_actually_works() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let home = tmp.path().join("forgedb-home");
    let project = home.join("projects").join("app");
    std::fs::create_dir_all(&project).unwrap();

    let absolute = tmp.path().join("var/lib/forgedb/data");
    let (ok, log) = run(&bin, &project, &home, &absolute.display().to_string());
    assert!(
        ok,
        "the absolute root the refusal recommends was itself refused:\n{log}"
    );
    assert!(log.contains("OPENED"), "{log}");
}

#[test]
#[cfg(unix)]
fn scenario_36_an_absolute_data_root_reaching_the_home_through_a_link_is_refused() {
    let tmp = TempDir::new().expect("tempdir");
    let bin = compile(tmp.path());

    let real_home = tmp.path().join("real-home");
    std::fs::create_dir_all(real_home.join("projects").join("app")).unwrap();
    let link = tmp.path().join("linked-home");
    std::os::unix::fs::symlink(&real_home, &link).unwrap();

    let through_the_link = link.join("projects").join("app").join("data");
    let outside = tmp.path().join("somewhere-else");
    std::fs::create_dir_all(&outside).unwrap();

    let (ok, log) = run(
        &bin,
        &outside,
        &real_home,
        &through_the_link.display().to_string(),
    );
    assert!(
        !ok,
        "an absolute data root reaching the ForgeDB home through a symlink was \
         allowed — the containment check is lexical, and the cache is writable \
         through the link:\n{log}"
    );
    assert!(
        log.contains("refusing to open a database inside the ForgeDB build cache"),
        "{log}"
    );
}
