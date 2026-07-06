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
        typescript: false,
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
        typescript: false,
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
