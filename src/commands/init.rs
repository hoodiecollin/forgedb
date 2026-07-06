use crate::{error::CliError, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub project_name: String,
    pub template: Option<String>,
    pub rust: bool,
    pub typescript: bool,
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
    // Create Cargo.toml with the dependencies required by the generated database.rs.
    // The generated code imports forgedb_storage, forgedb_types, derives serde traits,
    // and uses utoipa::ToSchema on Uuid/Timestamp fields (which requires the "uuid"
    // feature on utoipa, otherwise ToSchema fails to compile on those types).
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = "0.1"
forgedb-types = "0.1"
serde = {{ version = "1", features = ["derive"] }}
utoipa = {{ version = "5", features = ["uuid"] }}
"#,
        options.project_name
    );

    let cargo_path = Path::new(&options.project_name).join("Cargo.toml");
    fs::write(cargo_path, cargo_toml)?;

    // Create src/main.rs that uses generated code
    let main_rs = r#"// Include generated database code
mod generated {
    include!("../generated/database.rs");
}

use generated::*;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting ForgeDB application...");
    println!();

    // Initialize database
    let mut db = Database::new();

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
