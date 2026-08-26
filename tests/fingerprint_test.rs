//! #337 — the generated-source fingerprint.
//!
//! Scenarios 1–6 of the gate 2 plan (#353). 1–4 are the algorithm; 5–6 are its
//! *granularity*, and they drive the real CLI because granularity is a property
//! of what the emitter feeds the algorithm, not of the algorithm.
//!
//! Tier 1 throughout: nothing here compiles a crate.

use forgedb::fingerprint::{self, Entry, FINGERPRINT_FILE};
use std::path::{Path, PathBuf};
use std::process::Command;

fn e<'a>(path: &str, bytes: &'a str) -> Entry<'a> {
    Entry {
        path: path.to_string(),
        bytes,
    }
}

/// **Scenario 1 — order-independence, and the value is PINNED.**
///
/// The golden vector is the half that matters. Order-independence alone would
/// survive a switch to `DefaultHasher`, which is not stable across Rust releases
/// and would therefore move under a shim that is already committed — while
/// `cargo install` builds the CLI with the user's own toolchain, so the two
/// halves of one project could be hashed by two different algorithms.
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

/// **Scenario 2 — framing must be INJECTIVE.**
///
/// The gate 2 plan names `("a","bc")` vs `("ab","c")` as the collision. That
/// pair collides under *bare concatenation* — with the `\0` separator it does
/// not, so asserting only that pair leaves the length prefix unguarded, and a
/// mutation deleting the prefix survives. Verified by mutating it.
///
/// The separator marks the boundary between a path and its bytes. It does NOT
/// mark the boundary between one entry's bytes and the next entry's path, and
/// that unmarked seam is what the length prefix closes. Both pairs are asserted:
/// the plan's, because it must hold, and the real one, because only it has teeth.
#[test]
fn scenario_2_the_framing_is_injective() {
    assert_ne!(
        fingerprint::compute(&[e("a", "bc")]),
        fingerprint::compute(&[e("ab", "c")]),
        "`path + bytes` concatenation collides"
    );

    // The real collision. Without the length prefix both render `a\0bc\0`:
    // one entry whose body ends in a NUL, versus two entries whose seam falls
    // inside it.
    let one = [e("a", "bc\u{0}")];
    let two = [e("a", "b"), e("c", "")];
    assert_ne!(
        fingerprint::compute(&one),
        fingerprint::compute(&two),
        "the length prefix is not decoration: an entry's bytes running into the \
         next entry's path is a boundary the separator does not mark"
    );

    // A body containing a NUL must also not be confusable with a shorter one.
    assert_ne!(
        fingerprint::compute(&[e("a", "bc")]),
        fingerprint::compute(&[e("a", "\u{0}bc")]),
    );
}

/// **Scenario 3 — self-exclusion.** Adding the emitted constant file, carrying
/// the computed value, does not change the value.
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

/// **Scenario 4 — manifests count.** Every `.rs` byte identical, one substrate
/// pin different, and the fingerprints must differ: the pin changes the compiled
/// artifact while leaving the sources alone.
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

// ---------------------------------------------------------------------------
// Granularity — scenarios 5 and 6, driving the real CLI.
// ---------------------------------------------------------------------------

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

/// The app container in the cache. Derived by finding the ONE app directory —
/// never by joining a hash this test recomputes, which would be a second
/// derivation of `cache::member_hash` and would agree with it only by luck.
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

/// Read the emitted constant out of a cache package's `src/fingerprint.rs`.
///
/// The lookup PANICS on a miss rather than degrading: a scoping query that
/// silently returns nothing leaves the assertion live and aimed at the wrong
/// subject.
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

/// **Scenario 5 — the fingerprint is per (app, PACKAGE).**
///
/// Declaring a second binding must not invalidate the first one's artifact. A
/// per-app hash would: adding `python` changes nothing about the Node addon, and
/// forcing a rebuild whose only purpose is to restore agreement is how the check
/// trains people to ignore it.
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
    // …and the sibling that WAS added has its own, different value: a constant
    // that is equal everywhere would satisfy the assertion above vacuously.
    let pyo3 = emitted_fingerprint(&container(&dir, "s5"), "pyo3");
    assert_ne!(
        after, pyo3,
        "two packages share one fingerprint, so the equality above proves nothing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **Scenario 6 — class C is not an input.**
///
/// `transform-*` packages are written by `migrate build` into the same container.
/// If they were hashed, a data migration would invalidate every binding artifact
/// in the app.
#[test]
fn scenario_6_a_transform_package_does_not_change_a_binding_fingerprint() {
    let dir = project("s6", "\"rust\", \"node-runtime\"");
    ok(&forgedb(&dir, &["generate", "all"]), "generate all");
    let before = emitted_fingerprint(&container(&dir, "s6"), "napi");

    // Plant a class-C package in the container by hand. Driving `migrate build`
    // here would compile a crate (tier 2); what scenario 6 is about is whether
    // the hash INPUT reaches outside its own two directories, and a directory is
    // a directory however it got there.
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
