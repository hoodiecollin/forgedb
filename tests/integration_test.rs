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
        rust: true,
        api_only: false,
    };

    let result = run(options);
    assert!(result.is_ok(), "Init command should succeed");

    // Check that directories were created
    assert!(project_path.exists());
    assert!(project_path.join("src").exists());
    assert!(project_path.join("generated").exists());
    assert!(project_path.join("data/db").exists());

    // Check that files were created
    assert!(project_path.join("schema.forge").exists());
    assert!(project_path.join("forgedb.toml").exists());
    assert!(project_path.join(".gitignore").exists());
    assert!(project_path.join("README.md").exists());
    assert!(project_path.join("Cargo.toml").exists());
    assert!(project_path.join("src/main.rs").exists());
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
        rust: true,
        api_only: false,
    };

    let result = run(options);
    assert!(result.is_ok(), "Init with blog template should succeed");

    // Read schema and verify it contains blog-specific models
    let schema_content =
        fs::read_to_string(project_path.join("schema.forge")).expect("Failed to read schema");
    assert!(schema_content.contains("Post"));
    assert!(schema_content.contains("Tag"));
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

/// v1 Phase 4 (#92) Workstream 3: `migrate create --auto` diffs the schema against
/// a recorded snapshot and gates on additive-vs-breaking.  Additive (new nullable
/// field) is accepted; breaking (new non-null field) is refused with a non-zero
/// exit so CI catches it.  Hermetic: runs the real binary with an explicit cwd.
#[test]
fn test_migrate_auto_diff_additive_and_breaking_gate() {
    let temp_dir = setup_test_dir();
    let dir = temp_dir.path();

    let v1 = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
    let v2_additive = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";
    let v3_breaking =
        "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n  priority: u32\n}\n";
    fs::write(dir.join("v1.forge"), v1).unwrap();
    fs::write(dir.join("v2.forge"), v2_additive).unwrap();
    fs::write(dir.join("v3.forge"), v3_breaking).unwrap();

    // 1. Baseline records a snapshot with nothing to diff (succeeds, no migration).
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "baseline", "--auto", "--schema", "v1.forge"])
        .output()
        .expect("run migrate baseline");
    assert!(out.status.success(), "baseline should succeed");
    assert!(dir.join("migrations/.schema-snapshot.forge").exists(), "snapshot recorded");

    // 2. Additive (new nullable field) is accepted and a migration is written.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "add_note", "--auto", "--schema", "v2.forge"])
        .output()
        .expect("run migrate additive");
    assert!(out.status.success(), "additive migration should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("note"), "reports the added field: {stdout}");

    // 3. Breaking (new non-null field, no default) is refused with a non-zero exit.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "add_priority", "--auto", "--schema", "v3.forge"])
        .output()
        .expect("run migrate breaking");
    assert!(!out.status.success(), "breaking change must be refused (non-zero exit)");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("Breaking") && combined.to_lowercase().contains("reload"),
        "refusal explains the breaking change + reload path: {combined}"
    );
}
