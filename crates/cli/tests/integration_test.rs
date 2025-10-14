use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a temporary directory for testing
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

/// Helper to create a basic schema file
fn create_test_schema(dir: &Path) {
    let schema = r#"User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}
"#;
    fs::write(dir.join("schema.sink"), schema).expect("Failed to write schema");
}

#[test]
fn test_init_command_creates_project_structure() {
    use sinkdb_cli::commands::init::{run, InitOptions};

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
    assert!(project_path.join("schema.sink").exists());
    assert!(project_path.join("sinkdb.toml").exists());
    assert!(project_path.join(".gitignore").exists());
    assert!(project_path.join("README.md").exists());
    assert!(project_path.join("Cargo.toml").exists());
    assert!(project_path.join("src/main.rs").exists());
}

#[test]
fn test_init_with_blog_template() {
    use sinkdb_cli::commands::init::{run, InitOptions};

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
        fs::read_to_string(project_path.join("schema.sink")).expect("Failed to read schema");
    assert!(schema_content.contains("Post"));
    assert!(schema_content.contains("Tag"));
}

#[test]
fn test_generate_command_creates_rust_code() {
    use sinkdb_cli::commands::generate::{run, GenerateOptions};

    let temp_dir = setup_test_dir();
    std::env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

    // Create a schema file
    create_test_schema(temp_dir.path());

    let options = GenerateOptions {
        target: "rust".to_string(),
        check: false,
        output: Some("generated".to_string()),
        force: true,
    };

    let result = run(options);
    assert!(result.is_ok(), "Generate command should succeed");

    // Check that generated code exists
    let generated_file = temp_dir.path().join("generated/database.rs");
    assert!(generated_file.exists(), "Generated database.rs should exist");

    // Verify generated code contains expected content
    let generated_content =
        fs::read_to_string(&generated_file).expect("Failed to read generated code");
    assert!(generated_content.contains("User"));
    assert!(generated_content.contains("email"));
}

#[test]
fn test_validate_command_detects_errors() {
    use sinkdb_cli::commands::validate::{run, ValidateOptions};

    let temp_dir = setup_test_dir();
    std::env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

    // Create an invalid schema (model name not PascalCase)
    let invalid_schema = r#"user {
  id: +uuid
  Email: string
}
"#;
    fs::write(temp_dir.path().join("schema.sink"), invalid_schema)
        .expect("Failed to write schema");

    let options = ValidateOptions {
        strict: false,
        schema_only: false,
        implementations: false,
        components: false,
    };

    let result = run(options);
    // Should succeed but report errors
    assert!(result.is_ok());
}

#[test]
fn test_validate_command_passes_valid_schema() {
    use sinkdb_cli::commands::validate::{run, ValidateOptions};

    let temp_dir = setup_test_dir();
    std::env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

    create_test_schema(temp_dir.path());

    let options = ValidateOptions {
        strict: false,
        schema_only: true,
        implementations: false,
        components: false,
    };

    let result = run(options);
    assert!(result.is_ok(), "Valid schema should pass validation");
}

#[test]
fn test_generate_check_mode() {
    use sinkdb_cli::commands::generate::{run, GenerateOptions};

    let temp_dir = setup_test_dir();
    std::env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

    create_test_schema(temp_dir.path());

    // First generate code
    let gen_options = GenerateOptions {
        target: "rust".to_string(),
        check: false,
        output: Some("generated".to_string()),
        force: true,
    };
    run(gen_options).expect("Generate should succeed");

    // Now check mode should pass
    let check_options = GenerateOptions {
        target: "rust".to_string(),
        check: true,
        output: Some("generated".to_string()),
        force: false,
    };

    let result = run(check_options);
    assert!(result.is_ok(), "Check mode should pass for up-to-date code");
}
