use forgedb::fingerprint::{self, Entry, FINGERPRINT_FILE};
use std::path::{Path, PathBuf};
use std::process::Command;

fn e<'a>(path: &str, bytes: &'a str) -> Entry<'a> {
    Entry {
        path: path.to_string(),
        bytes,
    }
}

#[test]
fn scenario_1_order_does_not_change_the_value_and_the_value_is_pinned() {
    let forward = [
        e("core/Cargo.toml", "[package]\nname = \"a\"\n"),
        e("core/src/lib.rs", "pub fn a() {}\n"),
        e("napi/src/lib.rs", "pub fn b() {}\n"),
    ];
    let shuffled = [
        e("napi/src/lib.rs", "pub fn b() {}\n"),
        e("core/Cargo.toml", "[package]\nname = \"a\"\n"),
        e("core/src/lib.rs", "pub fn a() {}\n"),
    ];
    let value = fingerprint::compute(&forward);
    assert_eq!(
        value,
        fingerprint::compute(&shuffled),
        "the fingerprint depends on the order the emitter planned its files in"
    );
    assert_eq!(
        value, "223f35d6e7477e0b",
        "the fingerprint algorithm moved. This value is baked into committed \
         shims and into delivered artifacts; changing it invalidates every one \
         of them, so it is a deliberate, documented break — never an incidental \
         one."
    );
    assert_eq!(value.len(), 16, "16 lowercase hex digits, like the member hash");
    assert!(value.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
}

#[test]
fn scenario_2_the_framing_is_injective() {
    assert_ne!(
        fingerprint::compute(&[e("a", "bc")]),
        fingerprint::compute(&[e("ab", "c")]),
        "`path + bytes` concatenation collides"
    );

    let one = [e("a", "bc\u{0}")];
    let two = [e("a", "b"), e("c", "")];
    assert_ne!(
        fingerprint::compute(&one),
        fingerprint::compute(&two),
        "the length prefix is not decoration: an entry's bytes running into the \
         next entry's path is a boundary the separator does not mark"
    );

    assert_ne!(
        fingerprint::compute(&[e("a", "bc")]),
        fingerprint::compute(&[e("a", "\u{0}bc")]),
    );
}

#[test]
fn scenario_3_the_fingerprint_file_is_excluded_from_its_own_input() {
    let base = [
        e("napi/Cargo.toml", "[package]\n"),
        e("napi/src/lib.rs", "pub fn b() {}\n"),
    ];
    let value = fingerprint::compute(&base);

    let emitted = fingerprint::fingerprint_rs(&value);
    let with_self = [
        e("napi/Cargo.toml", "[package]\n"),
        e("napi/src/lib.rs", "pub fn b() {}\n"),
        e(&format!("napi/{FINGERPRINT_FILE}"), &emitted),
    ];
    assert_eq!(
        value,
        fingerprint::compute(&with_self),
        "the definition is circular — the file carrying the value is part of its own input"
    );
    assert!(
        emitted.contains(&format!("pub const FINGERPRINT: &str = \"{value}\";")),
        "the emitted file does not carry the value it was computed for:\n{emitted}"
    );
}

#[test]
fn scenario_4_a_substrate_pin_change_changes_the_fingerprint() {
    const LIB: &str = "pub fn a() {}\n";
    let before = [
        e("core/Cargo.toml", "[dependencies]\nforgedb-storage = \"0.3\"\n"),
        e("core/src/lib.rs", LIB),
    ];
    let after = [
        e("core/Cargo.toml", "[dependencies]\nforgedb-storage = \"0.4\"\n"),
        e("core/src/lib.rs", LIB),
    ];
    assert_ne!(
        fingerprint::compute(&before),
        fingerprint::compute(&after),
        "a manifest is not part of the input, so a pin bump reads as no change"
    );
}

const SCHEMA: &str = r#"
Author {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: ^string
  body: string
  author: *Author
}
"#;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn project(tag: &str, targets: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-fp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("schema.forge"), SCHEMA);
    write(
        &dir.join("forgedb.toml"),
        &format!(
            "[project]\nid = \"{tag}\"\n\n[generate]\ntargets = [{targets}]\n\n[storage]\nfsync = \"never\"\n"
        ),
    );
    dir
}

fn forgedb(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .args(args)
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".home"))
        .output()
        .expect("run forgedb")
}

fn ok(out: &std::process::Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn container(dir: &Path, name: &str) -> PathBuf {
    let apps = dir.join(".home/projects").join(name).join("apps");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no apps dir at {}: {e}", apps.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(found.len(), 1, "expected exactly one app under {}", apps.display());
    found.pop().unwrap()
}

fn emitted_fingerprint(container: &Path, package_dir: &str) -> String {
    let path = container.join(package_dir).join(FINGERPRINT_FILE);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no fingerprint at {}: {e}", path.display()));
    let open = src
        .find("FINGERPRINT: &str = \"")
        .unwrap_or_else(|| panic!("no FINGERPRINT constant in {}:\n{src}", path.display()))
        + "FINGERPRINT: &str = \"".len();
    let close = src[open..]
        .find('"')
        .unwrap_or_else(|| panic!("unterminated FINGERPRINT literal in {}", path.display()));
    src[open..open + close].to_string()
}

#[test]
fn scenario_5_declaring_another_target_does_not_change_a_sibling_fingerprint() {
    let dir = project("s5", "\"rust\", \"node-runtime\"");
    ok(&forgedb(&dir, &["generate", "all"]), "generate all (napi only)");
    let before = emitted_fingerprint(&container(&dir, "s5"), "napi");

    write(
        &dir.join("forgedb.toml"),
        "[project]\nid = \"s5\"\n\n[generate]\ntargets = [\"rust\", \"node-runtime\", \"python-runtime\"]\n\n[storage]\nfsync = \"never\"\n",
    );
    ok(
        &forgedb(&dir, &["generate", "all", "--force"]),
        "generate all (napi + pyo3)",
    );
    let after = emitted_fingerprint(&container(&dir, "s5"), "napi");

    assert_eq!(
        before, after,
        "adding an unrelated target invalidated the napi artifact — the fingerprint is per-app, not per-package"
    );
    let pyo3 = emitted_fingerprint(&container(&dir, "s5"), "pyo3");
    assert_ne!(
        after, pyo3,
        "two packages share one fingerprint, so the equality above proves nothing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scenario_6_a_transform_package_does_not_change_a_binding_fingerprint() {
    let dir = project("s6", "\"rust\", \"node-runtime\"");
    ok(&forgedb(&dir, &["generate", "all"]), "generate all");
    let before = emitted_fingerprint(&container(&dir, "s6"), "napi");

    let transform = container(&dir, "s6").join("transform-1-2");
    write(
        &transform.join("Cargo.toml"),
        "[package]\nname = \"planted\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(&transform.join("src/main.rs"), "fn main() {}\n");

    ok(&forgedb(&dir, &["generate", "all", "--force"]), "regenerate");
    assert_eq!(
        before,
        emitted_fingerprint(&container(&dir, "s6"), "napi"),
        "a class-C package in the container changed a binding's fingerprint"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
