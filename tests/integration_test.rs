use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Helper to create a temporary directory for testing
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

/// Build a `forgedb` CLI invocation scoped to `dir`.
///
/// Commands that auto-discover `schema.forge` do so from the process working
/// directory. Running the real binary as a subprocess with an explicit
/// `current_dir` gives each test its own working directory, so these tests are
/// hermetic and safe to run in parallel (no process-global `set_current_dir`).
fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    // #333: keep every claim this suite makes inside its own tempdir rather than
    // in the developer's real `~/.forgedb`.
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd.current_dir(dir);
    cmd
}

/// Helper to create a basic schema file
fn create_test_schema(dir: &Path) {
    let schema = r#"User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}
"#;
    fs::write(dir.join("schema.forge"), schema).expect("Failed to write schema");
}

#[test]
fn test_init_command_creates_project_structure() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "test_project";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: None,
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };

    let result = run(options);
    assert!(result.is_ok(), "Init command should succeed");

    // Check that directories were created
    assert!(project_path.exists());
    assert!(project_path.join("generated").exists());
    assert!(project_path.join("data/db").exists());

    // Check that files were created
    assert!(project_path.join("schema.forge").exists());
    assert!(project_path.join("forgedb.toml").exists());
    assert!(project_path.join(".gitignore").exists());
    assert!(project_path.join("README.md").exists());

    // ...and that the two the scaffold STOPPED writing are absent (#335 §15).
    // Asserted rather than merely deleted: `init` scaffolding no cargo package
    // is the change, and an absent assertion cannot notice it coming back.
    assert!(
        !project_path.join("Cargo.toml").exists(),
        "init scaffolded a cargo package; the generated Rust is compiled in \
         ForgeDB's build cache now"
    );
    assert!(
        !project_path.join("src").exists(),
        "init scaffolded a src/ directory; the server is a cache artifact"
    );
}

#[test]
fn test_init_with_blog_template() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "blog_project";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: Some("blog".to_string()),
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };

    let result = run(options);
    assert!(result.is_ok(), "Init with blog template should succeed");

    // Read schema and verify it contains blog-specific models
    let schema_content =
        fs::read_to_string(project_path.join("schema.forge")).expect("Failed to read schema");
    assert!(schema_content.contains("Post"));
    assert!(schema_content.contains("Tag"));
}

/// #115 on-host deploy: the Rust scaffold path emits the systemd unit, its
/// `EnvironmentFile`, and an install README under `deploy/`, alongside the Docker
/// artifacts. All name-templated only (identity: inert ops packaging), so the
/// unit's `StateDirectory` and the env file's `FORGEDB_DATA` must agree.
#[test]
fn test_init_emits_onhost_systemd_deploy() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "acme_svc";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: None,
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };
    run(options).expect("init should succeed");

    // The Docker path still emits (parity, not replaced).
    assert!(project_path.join("Dockerfile").exists());

    let deploy = project_path.join("deploy");
    let service =
        fs::read_to_string(deploy.join("acme_svc.service")).expect("systemd unit emitted");
    let env = fs::read_to_string(deploy.join("acme_svc.env")).expect("EnvironmentFile emitted");
    assert!(deploy.join("README.md").exists(), "install README emitted");

    // Load-bearing unit directives.
    assert!(service.contains("ExecStart=/usr/local/bin/acme_svc"));
    assert!(service.contains("EnvironmentFile=/etc/acme_svc/acme_svc.env"));
    assert!(service.contains("Type=exec"), "readiness model is exec, not notify");
    assert!(service.contains("DynamicUser=yes"), "non-root without a manual useradd");
    assert!(service.contains("StateDirectory=acme_svc"), "managed data dir");
    assert!(service.contains("KillSignal=SIGTERM"), "pairs with graceful shutdown");
    assert!(service.contains("NoNewPrivileges=yes"), "hardening present");
    assert!(service.contains("WantedBy=multi-user.target"));

    // The env file's data dir must match the unit's StateDirectory (/var/lib/<name>).
    assert!(env.contains("FORGEDB_DATA=/var/lib/acme_svc"), "env data dir agrees with StateDirectory");
    assert!(env.contains("FORGEDB_HOST=0.0.0.0"));

    // Identity guard: a single plain unit, never a templated <name>@.service that
    // would invite two writers against one data dir (the per-tenant template is
    // documented-only). No models/fields leak into the emitted artifacts.
    assert!(!deploy.join("acme_svc@.service").exists(), "no instance-template unit scaffolded");
}

#[test]
fn test_generate_command_creates_rust_code() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate");
    assert!(
        output.status.success(),
        "Generate command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check that generated code exists
    let generated_file = temp_dir.path().join("generated/database.rs");
    assert!(
        generated_file.exists(),
        "Generated database.rs should exist"
    );

    // Verify generated code contains expected content
    let generated_content =
        fs::read_to_string(&generated_file).expect("Failed to read generated code");
    assert!(generated_content.contains("User"));
    assert!(generated_content.contains("email"));
}

#[test]
fn test_validate_command_detects_errors() {
    let temp_dir = setup_test_dir();

    // Create an invalid schema (model name not PascalCase)
    let invalid_schema = r#"user {
  id: +uuid
  Email: string
}
"#;
    fs::write(temp_dir.path().join("schema.forge"), invalid_schema)
        .expect("Failed to write schema");

    let output = forgedb_cmd(temp_dir.path())
        .arg("validate")
        .output()
        .expect("Failed to run forgedb validate");
    // Should fail due to validation errors in the parsing phase
    assert!(
        !output.status.success(),
        "Invalid schema should fail validation"
    );
}

#[test]
fn test_validate_command_passes_valid_schema() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    let output = forgedb_cmd(temp_dir.path())
        .args(["validate", "--schema-only"])
        .output()
        .expect("Failed to run forgedb validate");
    assert!(
        output.status.success(),
        "Valid schema should pass validation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// C1: `forgedb build` must be idempotent — a second consecutive run must not
/// fail with "File exists" on already-generated output files.
///
/// The `build` command internally calls `generate all` with `force: true` so
/// that generated artifacts are always overwritten rather than erroring on an
/// existing file.  We verify this by testing the generate layer directly:
///
/// - `generate all` without `--force` fails on a second run (baseline for why
///   the bug existed).
/// - `generate all --force` succeeds on a second run (proof of the fix, which
///   is exactly what `build` now does internally).
///
/// We cannot run `forgedb build` end-to-end in a hermetic temp dir because
/// `build` also invokes `cargo build` as a subprocess, which requires a full
/// Rust workspace — that is separate infrastructure from the C1 fix.
#[test]
fn test_generate_all_is_idempotent_with_force() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    // First run — succeeds unconditionally.
    let first = forgedb_cmd(temp_dir.path())
        .args(["generate", "all", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate all (first run)");
    assert!(
        first.status.success(),
        "First generate all --force should succeed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    // Second run with --force — must also succeed (files already exist).
    // This is what `forgedb build` now does internally (force: true).
    let second = forgedb_cmd(temp_dir.path())
        .args(["generate", "all", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate all (second run)");
    assert!(
        second.status.success(),
        "Second generate all --force should succeed (idempotency — C1 fix): {}",
        String::from_utf8_lossy(&second.stderr)
    );

    // Confirm the failure mode: without --force the second run must fail.
    let third_no_force = forgedb_cmd(temp_dir.path())
        .args(["generate", "all", "--output", "generated"])
        .output()
        .expect("Failed to run forgedb generate all (no-force third run)");
    assert!(
        !third_no_force.status.success(),
        "generate all without --force should fail when files already exist"
    );
}

#[test]
fn test_generate_check_mode() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    // First generate code
    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate");
    assert!(
        output.status.success(),
        "Generate should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Now check mode should pass for up-to-date code
    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--check"])
        .output()
        .expect("Failed to run forgedb generate --check");
    assert!(
        output.status.success(),
        "Check mode should pass for up-to-date code: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// #74 Phase 4: `migrate create --auto` diffs the schema against a recorded
/// snapshot, records EVERY non-empty diff as a versioned hop, and classifies the
/// hop body.  A purely-additive change (new nullable field) succeeds with the
/// reopen-backfill fast-path advice; a change whose new-row value the differ
/// cannot prove (a type change) succeeds too but is classified `Authored`, gets a
/// `migrations/<id>/transform.rs` scaffold, and prints the `migrate up` next
/// steps.  The old refuse-breaking gate is gone.  Hermetic: real binary, explicit
/// cwd.
#[test]
fn test_migrate_auto_records_and_scaffolds_authored_hop() {
    let temp_dir = setup_test_dir();
    let dir = temp_dir.path();

    let v1 = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
    let v2_additive = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";
    // Breaking + Authored: `qty` changes u32 -> string (the differ cannot know the
    // re-encoding, so the developer must author it).
    let v3_authored =
        "Widget {\n  id: +uuid\n  sku: &string\n  qty: string\n  note: string?\n}\n";
    fs::write(dir.join("v1.forge"), v1).unwrap();
    fs::write(dir.join("v2.forge"), v2_additive).unwrap();
    fs::write(dir.join("v3.forge"), v3_authored).unwrap();

    // 1. Baseline records a snapshot with nothing to diff (succeeds, no migration).
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "baseline", "--auto", "--schema", "v1.forge"])
        .output()
        .expect("run migrate baseline");
    assert!(out.status.success(), "baseline should succeed");
    assert!(dir.join("migrations/.schema-snapshot.forge").exists(), "snapshot recorded");
    assert!(dir.join("migrations/schemas/v1.forge").exists(), "baseline versioned schema recorded");

    // 2. Additive (new nullable field) succeeds; the fast-path advice mentions
    //    backfill-on-reopen, and the destination version schema is recorded.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "add_note", "--auto", "--schema", "v2.forge"])
        .output()
        .expect("run migrate additive");
    assert!(out.status.success(), "additive migration should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("note"), "reports the added field: {stdout}");
    assert!(
        stdout.to_lowercase().contains("backfill"),
        "additive prints the reopen fast-path advice: {stdout}"
    );
    assert!(dir.join("migrations/schemas/v2.forge").exists(), "v2 versioned schema recorded");

    // 3. A type change (u32 -> string) is now RECORDED (non-zero exit gone),
    //    classified Authored, scaffolded, and prints the `migrate up` steps.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "qty_to_string", "--auto", "--schema", "v3.forge"])
        .output()
        .expect("run migrate authored");
    assert!(
        out.status.success(),
        "authored migration is recorded, not refused: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(combined.contains("[authored]"), "marks the authored hop: {combined}");
    // `migrate up` was removed (#335 §9): the next steps name the two commands
    // it wrapped, and both are now `--schema`-addressed.
    assert!(
        !combined.contains("migrate up"),
        "still teaches the removed `migrate up`: {combined}"
    );
    assert!(
        combined.contains("migrate build") && combined.contains("migrate run"),
        "does not print the build+run next steps: {combined}"
    );
    assert!(dir.join("migrations/schemas/v3.forge").exists(), "v3 versioned schema recorded");

    // The authored scaffold was written under migrations/<id>/transform.rs.
    let scaffolds: Vec<_> = fs::read_dir(dir.join("migrations"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("transform.rs").exists())
        .collect();
    assert_eq!(scaffolds.len(), 1, "exactly one authored-body scaffold written");
    let body = fs::read_to_string(scaffolds[0].join("transform.rs")).unwrap();
    assert!(
        body.contains("authored_transform") && body.contains("TODO"),
        "scaffold is the fill-in-the-TODO authored transform: {body}"
    );
}
