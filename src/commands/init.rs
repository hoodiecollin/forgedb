use crate::{error::CliError, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub project_name: String,
    pub template: Option<String>,
    pub rust: bool,
    pub api_only: bool,
}

pub fn run(options: InitOptions) -> Result<()> {
    ui::header("✨", &format!("Creating project: {}", options.project_name));

    // Check if project directory already exists
    let project_path = Path::new(&options.project_name);
    if project_path.exists() {
        return Err(CliError::ProjectExists(options.project_name.clone()));
    }

    // Create project directory structure
    create_project_structure(&options)?;

    // Create schema file based on template
    create_schema_file(&options)?;

    // Create config file
    create_config_file(&options)?;

    // Create .gitignore
    create_gitignore(&options)?;

    // Create README
    create_readme(&options)?;

    // Create Rust files if needed
    if options.rust || !options.api_only {
        create_rust_files(&options)?;
    }

    ui::success("Done! Run the following to get started:");
    println!();
    println!("  cd {}", options.project_name);
    println!("  forgedb generate rust");
    println!("  forgedb build");
    println!();

    Ok(())
}

fn create_project_structure(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);

    // Create main directories
    fs::create_dir_all(project_path)?;
    fs::create_dir_all(project_path.join("src"))?;
    fs::create_dir_all(project_path.join("generated"))?;
    fs::create_dir_all(project_path.join("data/db"))?;
    fs::create_dir_all(project_path.join("data/wal"))?;

    ui::success("Created project directory structure");
    Ok(())
}

fn create_schema_file(options: &InitOptions) -> Result<()> {
    let schema_content = match options.template.as_deref() {
        Some("blog") => templates::blog_schema(),
        Some("ecommerce") => templates::ecommerce_schema(),
        Some("todo") => templates::todo_schema(),
        Some("blank") | None => templates::blank_schema(),
        Some(t) => {
            ui::warning(&format!("Unknown template '{}', using blank", t));
            templates::blank_schema()
        }
    };

    let schema_path = Path::new(&options.project_name).join("schema.forge");
    fs::write(schema_path, schema_content)?;

    ui::step("📄", "Created schema.forge");
    Ok(())
}

fn create_config_file(options: &InitOptions) -> Result<()> {
    let config_content = templates::default_config(&options.project_name);
    let config_path = Path::new(&options.project_name).join("forgedb.toml");
    fs::write(config_path, config_content)?;

    ui::step("⚙️", "Created forgedb.toml");
    Ok(())
}

fn create_gitignore(options: &InitOptions) -> Result<()> {
    let gitignore_path = Path::new(&options.project_name).join(".gitignore");
    fs::write(gitignore_path, templates::default_gitignore())?;

    ui::step("📝", "Created .gitignore");
    Ok(())
}

fn create_readme(options: &InitOptions) -> Result<()> {
    let readme_content = templates::readme_template(&options.project_name);
    let readme_path = Path::new(&options.project_name).join("README.md");
    fs::write(readme_path, readme_content)?;

    ui::step("📖", "Created README.md");
    Ok(())
}

fn create_rust_files(options: &InitOptions) -> Result<()> {
    // Create Cargo.toml with all dependencies required by generated code.
    //
    // The generated `database.rs` needs: forgedb-storage, forgedb-types, serde,
    // utoipa (with uuid feature for ToSchema on Uuid/Timestamp fields).
    //
    // The generated `api.rs` needs: axum, utoipa-axum, tokio (full), serde_json.
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = "0.1.2"
forgedb-types = "0.2"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
utoipa = {{ version = "5", features = ["uuid"] }}
utoipa-axum = "0.2"
axum = "0.8"
tokio = {{ version = "1", features = ["full"] }}
"#,
        options.project_name
    );

    let cargo_path = Path::new(&options.project_name).join("Cargo.toml");
    fs::write(cargo_path, cargo_toml)?;

    // Create src/main.rs that uses generated code
    let main_rs = r#"// The generated `generated/database.rs` is a module file with its own inner
// attributes (`#![allow(...)]`) and `//!` docs, so it must be pulled in as a
// `#[path]` module — NOT via `include!` inside an inline `mod { }`, which
// rejects inner attributes (E0753).
#[path = "../generated/database.rs"]
mod generated;

use generated::Database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting ForgeDB application...");
    println!();

    // Initialize the generated database.
    let _db = Database::new();

    println!("✅ Database initialized successfully!");
    println!();
    println!("You can now:");
    println!("  - Add data operations in src/main.rs");
    println!("  - Update schema.forge and regenerate with: forgedb generate rust");
    println!();

    Ok(())
}
"#;

    let main_rs_path = Path::new(&options.project_name).join("src").join("main.rs");
    fs::write(main_rs_path, main_rs)?;

    ui::step("🦀", "Created Rust project files");
    ui::info("Run 'forgedb generate --rust' to generate the database code");
    Ok(())
}
