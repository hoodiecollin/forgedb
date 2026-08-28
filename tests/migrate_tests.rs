use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const V1: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
const V2: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";
const V3: &str =
    "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n  color: string?\n}\n";

const PROJECT: &str = "migrate-scenarios";

const CONFIG: &str = "[project]\nid = \"migrate-scenarios\"\n\n[generate]\ntargets = [\"all\"]\n";

fn forgedb(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir).env("FORGEDB_HOME", home(dir));
    cmd
}

fn home(dir: &Path) -> PathBuf {
    dir.join(".forgedb-home")
}

fn project_root(dir: &Path) -> PathBuf {
    home(dir).join("projects").join(PROJECT)
}

fn container(dir: &Path) -> PathBuf {
    let apps = project_root(dir).join("apps");
    let mut found: Vec<PathBuf> = fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("no app container under {}: {e}", apps.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one app container under {}, found {found:?}",
        apps.display()
    );
    found.pop().unwrap()
}

fn package_name(dir: &Path, kind: &str) -> String {
    let app = forgedb::cache::member_app_name(&container(dir))
        .expect("the container records an app-name marker");
    format!("{app}-{kind}")
}

fn record_lineage(dir: &Path) {
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    for (name, body) in [("baseline", V1), ("add_note", V2), ("add_color", V3)] {
        fs::write(dir.join("schema.forge"), body).unwrap();
        let out = forgedb(dir)
            .args([
                "migrate",
                "create",
                name,
                "--schema",
                "schema.forge",
            ])
            .output()
            .expect("run migrate create");
        assert!(
            out.status.success(),
            "recording the {name} hop failed:\n{}",
            combined(&out)
        );
    }
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn test_scenario_32_every_migrate_arm_requires_schema() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    fs::write(dir.join("schema.forge"), V1).unwrap();

    let arms: [&[&str]; 5] = [
        &["migrate", "create", "some-change"],
        &["migrate", "status"],
        &["migrate", "build", "--from", "1", "--to", "2"],
        &[
            "migrate", "run", "--from", "1", "--to", "2", "--src", "a", "--dest", "b",
        ],
        &["migrate", "engine", "--src", "a", "--dest", "b"],
    ];

    for arm in arms {
        let out = forgedb(dir).args(arm).output().expect("run forgedb");
        let log = combined(&out);
        assert!(
            !out.status.success(),
            "`forgedb {}` ran without naming an app:\n{log}",
            arm.join(" ")
        );
        assert!(
            log.contains("--schema"),
            "`forgedb {}` must refuse by naming --schema:\n{log}",
            arm.join(" ")
        );
    }
}

#[test]
fn test_scenario_32_a_missing_transformer_names_the_cache_and_never_falls_back() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    let out = forgedb(dir)
        .args([
            "migrate",
            "run",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
            "--src",
            "data",
            "--dest",
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);

    assert!(!out.status.success(), "migrate run invented a bin:\n{log}");
    assert!(
        log.contains(&container(dir).display().to_string()),
        "the error must name the cache member it looked for:\n{log}"
    );
    assert!(
        log.contains("transform-1-2"),
        "and it must name the RANGE, since that is what selects the member:\n{log}"
    );
    assert!(
        !log.contains("migrations/transform"),
        "no fallback to the path that reproduces #328:\n{log}"
    );
    assert!(
        !dir.join("migrations/transform").exists(),
        "nothing may be emitted into the user's tree:\n{log}"
    );
}

#[test]
#[cfg(unix)]
fn test_scenario_33_run_resolves_the_named_range_member() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    let out = forgedb(dir)
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .expect("run migrate status");
    assert!(
        out.status.success(),
        "migrate status failed:\n{}",
        combined(&out)
    );

    let container = container(dir);
    let bindir = project_root(dir).join("target/release");
    fs::create_dir_all(&bindir).unwrap();

    let mut markers = Vec::new();
    for range in ["1-2", "2-3"] {
        let member = container.join(format!("transform-{range}"));
        fs::create_dir_all(&member).unwrap();
        fs::write(member.join("Cargo.toml"), "[package]\nname = \"stub\"\n").unwrap();

        let marker = dir.join(format!("ran-{range}"));
        let bin = bindir.join(package_name(dir, &format!("transform-{range}")));
        fs::write(
            &bin,
            format!("#!/bin/sh\necho \"$@\" > {}\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        markers.push(marker);
    }
    let (marker_1_2, marker_2_3) = (markers[0].clone(), markers[1].clone());

    let out = forgedb(dir)
        .args([
            "migrate",
            "run",
            "--schema",
            "schema.forge",
            "--from",
            "2",
            "--to",
            "3",
            "--src",
            "data",
            "--dest",
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(
        marker_2_3.is_file(),
        "the 2->3 transformer was named and did not run:\n{log}"
    );
    assert!(
        !marker_1_2.is_file(),
        "the 1->2 transformer ran instead of the one that was named:\n{log}"
    );

    fs::remove_file(&marker_2_3).unwrap();
    let out = forgedb(dir)
        .args([
            "migrate",
            "run",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
            "--src",
            "data",
            "--dest",
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(marker_1_2.is_file(), "the 1->2 range did not run:\n{log}");
    assert!(
        !marker_2_3.is_file(),
        "the 2->3 transformer ran for the 1->2 range:\n{log}"
    );
}

#[test]
fn test_migrate_build_emits_a_range_stamped_cache_member() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    record_lineage(dir);

    let out = forgedb(dir)
        .env("CARGO_NET_OFFLINE", "true")
        .args([
            "migrate",
            "build",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);

    let member = container(dir).join("transform-1-2");
    assert!(
        member.join("Cargo.toml").is_file(),
        "the transformer must be emitted as a cache member:\n{log}"
    );
    assert!(
        member.join("src/main.rs").is_file(),
        "and its sources with it:\n{log}"
    );
    assert!(
        !dir.join("migrations/transform").exists(),
        "and nothing may land in the user's tree:\n{log}"
    );

    let manifest = fs::read_to_string(member.join("Cargo.toml")).unwrap();
    let pkg = package_name(dir, "transform-1-2");
    assert!(
        manifest.contains(&format!("name = \"{pkg}\"")),
        "the member must be named for its app AND its range:\n{manifest}"
    );
    let bin_section = manifest
        .split("[[bin]]")
        .nth(1)
        .unwrap_or_else(|| panic!("no [[bin]] section:\n{manifest}"));
    assert!(
        bin_section.contains(&format!("name = \"{pkg}\"")),
        "the [[bin]] must carry the range too:\n{manifest}"
    );
    assert!(
        !manifest.contains("[workspace]"),
        "a member declaring its own workspace leaves the shared lockfile and target/:\n{manifest}"
    );
    assert!(
        !manifest.contains("[profile"),
        "cargo ignores a profile in a non-root member — shipping one is a setting that \
         reads as applied and is not:\n{manifest}"
    );

    let root = fs::read_to_string(project_root(dir).join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("no cache workspace root:\n{e}\n{log}"));
    assert!(
        root.contains("transform-1-2"),
        "the workspace root must name the member that was just emitted:\n{root}"
    );
}

#[test]
#[cfg(unix)]
fn test_migrate_run_honours_a_redirected_target_dir() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let redirected = dir.join("elsewhere-target");
    record_lineage(dir);

    let out = forgedb(dir)
        .args(["migrate", "status", "--schema", "schema.forge"])
        .output()
        .expect("run migrate status");
    assert!(
        out.status.success(),
        "migrate status failed:\n{}",
        combined(&out)
    );

    let container = container(dir);
    let member = container.join("transform-1-2");
    let pkg = package_name(dir, "transform-1-2");
    fs::create_dir_all(member.join("src")).unwrap();
    fs::write(
        member.join("Cargo.toml"),
        format!("[package]\nname = \"{pkg}\"\nversion = \"0.0.0\"\nedition = \"2021\"\nautobins = false\n\n[[bin]]\nname = \"{pkg}\"\npath = \"src/main.rs\"\n"),
    )
    .unwrap();
    fs::write(member.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        project_root(dir).join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"3\"\nmembers = [\"apps/{}/transform-1-2\"]\n",
            container.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();

    let marker = dir.join("ran-redirected");
    let bindir = redirected.join("release");
    fs::create_dir_all(&bindir).unwrap();
    let bin = bindir.join(&pkg);
    fs::write(
        &bin,
        format!("#!/bin/sh\necho \"$@\" > {}\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!project_root(dir).join("target/release").join(&pkg).exists());

    let out = forgedb(dir)
        .env("CARGO_TARGET_DIR", &redirected)
        .args([
            "migrate",
            "run",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
            "--src",
            "data",
            "--dest",
            "data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);

    assert!(
        !log.contains("not found"),
        "migrate run sends the user back to a build that already happened:\n{log}"
    );
    assert!(out.status.success(), "migrate run failed:\n{log}");
    assert!(
        marker.is_file(),
        "the transformer in the redirected target dir never ran:\n{log}"
    );
}

#[test]
#[ignore = "compiles a real transformer against the published substrate (network + minutes)"]
fn test_migrate_build_reports_the_path_cargo_actually_wrote() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let redirected = dir.join("elsewhere-target");
    record_lineage(dir);

    let out = forgedb(dir)
        .env("CARGO_TARGET_DIR", &redirected)
        .args([
            "migrate",
            "build",
            "--schema",
            "schema.forge",
            "--from",
            "1",
            "--to",
            "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);

    let reported = log
        .lines()
        .find_map(|l| l.split_once("Built transformer:").map(|(_, p)| p.trim()))
        .unwrap_or_else(|| panic!("migrate build never reported a transformer:\n{log}"));
    let reported = PathBuf::from(strip_ansi(reported));

    assert!(
        reported.is_file(),
        "migrate build reported {} and nothing is there — #292 verbatim:\n{log}",
        reported.display()
    );
    assert!(
        reported.starts_with(&redirected),
        "the reported path {} is not under the redirected target dir {} — it was \
         joined by hand rather than read from cargo:\n{log}",
        reported.display(),
        redirected.display()
    );

    let pkg = package_name(dir, "transform-1-2");
    let unredirected = project_root(dir).join("target/release").join(&pkg);
    assert!(
        !unredirected.exists(),
        "a binary appeared at the un-redirected guess {}:\n{log}",
        unredirected.display()
    );
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

#[test]
fn test_migrate_spawns_no_cargo_of_its_own() {
    let src = include_str!("../src/commands/migrate/mod.rs");
    let offenders: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("Command::new(\"cargo\")"))
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "migrate.rs spawns cargo directly again — route it through \
         crate::commands::build::driver instead:\n{}",
        offenders
            .iter()
            .map(|(n, l)| format!("  {n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert!(
        src.contains("driver::execute(&driver::plan("),
        "migrate.rs no longer builds through driver::plan + driver::execute"
    );
    assert!(
        src.contains("driver::assert_no_duplicate_artifact_names"),
        "migrate.rs no longer runs the pre-build collision guard, so a \
         transform/engine bin-name collision would be a cargo WARNING at exit 0"
    );
    assert!(
        src.contains("driver::target_directory("),
        "migrate run no longer asks the driver where cargo writes"
    );
}

const ENUM_V1: &str = "enum Status { Draft  Published  Archived }\n\
                       Post {\n  id: +uuid\n  title: string\n  status: Status\n}\n";
const ENUM_V2: &str = "enum Status { Published  Draft  Archived }\n\
                       Post {\n  id: +uuid\n  title: string\n  status: Status\n}\n";

#[test]
fn test_an_enum_reorder_records_a_hop_and_leaves_v1_intact() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();

    fs::write(dir.join("schema.forge"), ENUM_V1).unwrap();
    let baseline = forgedb(dir)
        .args([
            "migrate",
            "create",
            "baseline",
            "--schema",
            "schema.forge",
        ])
        .output()
        .expect("run migrate create");
    assert!(
        baseline.status.success(),
        "baseline:\n{}",
        combined(&baseline)
    );

    fs::write(dir.join("schema.forge"), ENUM_V2).unwrap();
    let reorder = forgedb(dir)
        .args([
            "migrate",
            "create",
            "swap_status",
            "--schema",
            "schema.forge",
        ])
        .output()
        .expect("run migrate create");
    assert!(reorder.status.success(), "reorder:\n{}", combined(&reorder));

    let said = strip_ansi(&combined(&reorder));
    assert!(
        !said.contains("No schema changes detected"),
        "the reorder went unseen — this is #438 verbatim:\n{said}"
    );

    let records: Vec<PathBuf> = fs::read_dir(dir.join("migrations"))
        .expect("migrations/ exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(
        records.len(),
        1,
        "expected exactly one recorded migration, got {records:?}\n{said}"
    );

    let body = fs::read_to_string(&records[0]).unwrap();
    let record: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(record["from_version"], 1, "from_version:\n{body}");
    assert_eq!(record["to_version"], 2, "to_version:\n{body}");

    let v1 = fs::read_to_string(dir.join("migrations/schemas/v1.forge"))
        .expect("v1.forge was recorded at baseline");
    assert!(
        v1.contains("Draft  Published"),
        "migrations/schemas/v1.forge was overwritten with the NEW variant order. \
         The lineage now asserts that v1 — the version the existing rows were \
         written under — always had the new ordering, and the transformer would \
         reproduce the corruption faithfully. Got:\n{v1}"
    );
    let v2 = fs::read_to_string(dir.join("migrations/schemas/v2.forge"))
        .expect("v2.forge is recorded for the destination version");
    assert!(v2.contains("Published  Draft"), "v2.forge:\n{v2}");
}

const NESTED_V1: &str = "enum Color { Red  Green  Blue }\n\n\
                         struct Badge {\n  rank: u32\n  tint: Color\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";
const NESTED_V2: &str = "enum Color { Green  Red  Blue }\n\n\
                         struct Badge {\n  rank: u32\n  tint: Color\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";
const NESTED_V3: &str = "enum Color { Green  Red  Blue }\n\n\
                         struct Badge {\n  tint: Color\n  rank: u32\n}\n\n\
                         Sticker {\n  id: +uuid\n  badges: [Badge; 4]\n}\n";

#[test]
fn test_a_nested_enum_and_struct_change_reach_the_differ() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();

    let create = |body: &str, desc: &str| {
        fs::write(dir.join("schema.forge"), body).unwrap();
        let out = forgedb(dir)
            .args([
                "migrate",
                "create",
                desc,
                "--schema",
                "schema.forge",
            ])
            .output()
            .expect("run migrate create");
        assert!(out.status.success(), "{desc}:\n{}", combined(&out));
        strip_ansi(&combined(&out))
    };

    create(NESTED_V1, "baseline");

    let reorder_enum = create(NESTED_V2, "swap_color");
    assert!(
        reorder_enum.contains("Enum 'Color'") && reorder_enum.contains("Sticker.badges"),
        "an enum nested inside a struct inside a fixed array must project onto the \
         model field that stores it:\n{reorder_enum}"
    );

    let reorder_struct = create(NESTED_V3, "swap_badge_fields");
    assert!(
        reorder_struct.contains("Struct 'Badge'") && reorder_struct.contains("Sticker.badges"),
        "a struct field reorder must be reported against the storing field:\n{reorder_struct}"
    );
}
