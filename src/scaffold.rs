use std::fs;
use std::path::PathBuf;
use std::io;

pub struct ScaffoldConfig {
    pub project_name: String,
    pub project_path: PathBuf,
}

impl ScaffoldConfig {
    pub fn new(project_name: String) -> Self {
        let project_path = PathBuf::from(&project_name);
        Self {
            project_name,
            project_path,
        }
    }
}

pub struct Scaffolder {
    config: ScaffoldConfig,
}

impl Scaffolder {
    pub fn new(config: ScaffoldConfig) -> Self {
        Self { config }
    }

    pub fn scaffold(&self) -> io::Result<()> {
        // Check if directory already exists
        if self.config.project_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Directory '{}' already exists", self.config.project_name),
            ));
        }

        // Create project directory
        fs::create_dir_all(&self.config.project_path)?;

        // Create subdirectories
        self.create_directories()?;

        // Generate files
        self.generate_schema_file()?;
        self.generate_config_file()?;
        self.generate_gitignore()?;
        self.generate_cargo_toml()?;
        self.generate_main_rs()?;
        self.generate_readme()?;

        Ok(())
    }

    fn create_directories(&self) -> io::Result<()> {
        let src_dir = self.config.project_path.join("src");
        fs::create_dir_all(&src_dir)?;
        Ok(())
    }

    fn generate_schema_file(&self) -> io::Result<()> {
        let schema_path = self.config.project_path.join("schema.sink");
        let schema_content = self.get_schema_template();
        fs::write(schema_path, schema_content)?;
        Ok(())
    }

    fn generate_config_file(&self) -> io::Result<()> {
        let config_path = self.config.project_path.join("sinkdb.toml");
        let config_content = self.get_config_template();
        fs::write(config_path, config_content)?;
        Ok(())
    }

    fn generate_gitignore(&self) -> io::Result<()> {
        let gitignore_path = self.config.project_path.join(".gitignore");
        let gitignore_content = self.get_gitignore_template();
        fs::write(gitignore_path, gitignore_content)?;
        Ok(())
    }

    fn generate_cargo_toml(&self) -> io::Result<()> {
        let cargo_path = self.config.project_path.join("Cargo.toml");
        let cargo_content = self.get_cargo_template();
        fs::write(cargo_path, cargo_content)?;
        Ok(())
    }

    fn generate_main_rs(&self) -> io::Result<()> {
        let main_path = self.config.project_path.join("src").join("main.rs");
        let main_content = self.get_main_template();
        fs::write(main_path, main_content)?;
        Ok(())
    }

    fn generate_readme(&self) -> io::Result<()> {
        let readme_path = self.config.project_path.join("README.md");
        let readme_content = self.get_readme_template();
        fs::write(readme_path, readme_content)?;
        Ok(())
    }

    fn get_schema_template(&self) -> String {
        format!(r#"// {} Schema
// Define your data models here

User {{
  id: +uuid
  email: ^&string @email
  username: ^string
  created_at: +timestamp
}}
"#, self.config.project_name)
    }

    fn get_config_template(&self) -> String {
        format!(r#"# SinkDB Configuration
[project]
name = "{}"
version = "0.1.0"

[database]
# Database storage path (relative to project root)
path = "./data"

# Schema file location
schema = "./schema.sink"

# Generated code output directory
output = "./generated"

[server]
# API server configuration (optional)
enabled = false
host = "127.0.0.1"
port = 3000

[watch]
# Enable file watching for auto-regeneration
enabled = true

# Debounce delay in milliseconds
debounce_ms = 100
"#, self.config.project_name)
    }

    fn get_gitignore_template(&self) -> String {
        r#"# Rust
/target
Cargo.lock
**/*.rs.bk
*.pdb

# SinkDB
/generated
/data
*.db
*.log

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db

# Logs
*.log
logs/
"#.to_string()
    }

    fn get_cargo_template(&self) -> String {
        format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
# Add sinkdb runtime dependencies here
# sinkdb-runtime = "0.1"

[[bin]]
name = "{}"
path = "src/main.rs"
"#, self.config.project_name, self.config.project_name)
    }

    fn get_main_template(&self) -> String {
        r#"// Import generated database code
// mod generated;

fn main() {
    println!("Welcome to your SinkDB project!");
    println!("");
    println!("Next steps:");
    println!("  1. Edit schema.sink to define your data models");
    println!("  2. Run 'sinkdb generate' to generate database code");
    println!("  3. Import and use the generated code in your application");
    println!("");
    println!("For more information, see README.md");
}
"#.to_string()
    }

    fn get_readme_template(&self) -> String {
        format!(r#"# {}

A SinkDB database project.

## Getting Started

### 1. Define Your Schema

Edit `schema.sink` to define your data models:

```sink
User {{
  id: +uuid
  email: ^&string @email
  username: ^string
  created_at: +timestamp
}}
```

### 2. Generate Database Code

```bash
sinkdb generate
```

This will parse your schema and generate type-safe Rust code in the `generated/` directory.

### 3. Use the Generated Code

```rust
mod generated;
use generated::Database;

fn main() {{
    let mut db = Database::new("./data").unwrap();

    // Create a user
    let user_id = db.users.insert(
        "user@example.com".to_string(),
        "johndoe".to_string()
    ).unwrap();

    // Query users
    let user = db.users.get(user_id).unwrap();
    println!("User: {{:?}}", user);
}}
```

## Commands

- `sinkdb generate` - Generate database code from schema
- `sinkdb validate` - Validate schema without generating code
- `sinkdb watch` - Watch schema file and auto-regenerate on changes
- `sinkdb build` - Generate and compile code

## Configuration

See `sinkdb.toml` for configuration options.

## Learn More

- [SinkDB Documentation](https://github.com/yourusername/sinkdb)
- [Schema Language Reference](https://github.com/yourusername/sinkdb/docs/schema.md)
- [API Reference](https://github.com/yourusername/sinkdb/docs/api.md)
"#, self.config.project_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scaffold_creates_project_directory() {
        let temp_dir = std::env::temp_dir().join("sinkdb_test_scaffold_basic");
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).ok();
        }

        let config = ScaffoldConfig {
            project_name: "test_project".to_string(),
            project_path: temp_dir.clone(),
        };

        let scaffolder = Scaffolder::new(config);
        assert!(scaffolder.scaffold().is_ok());
        assert!(temp_dir.exists());
        assert!(temp_dir.is_dir());

        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_scaffold_creates_required_files() {
        let temp_dir = std::env::temp_dir().join("sinkdb_test_scaffold_files");
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).ok();
        }

        let config = ScaffoldConfig {
            project_name: "test_project".to_string(),
            project_path: temp_dir.clone(),
        };

        let scaffolder = Scaffolder::new(config);
        scaffolder.scaffold().unwrap();

        // Check required files exist
        assert!(temp_dir.join("schema.sink").exists());
        assert!(temp_dir.join("sinkdb.toml").exists());
        assert!(temp_dir.join(".gitignore").exists());
        assert!(temp_dir.join("Cargo.toml").exists());
        assert!(temp_dir.join("src/main.rs").exists());
        assert!(temp_dir.join("README.md").exists());

        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_scaffold_rejects_existing_directory() {
        let temp_dir = std::env::temp_dir().join("sinkdb_test_scaffold_exists");
        fs::create_dir_all(&temp_dir).unwrap();

        let config = ScaffoldConfig {
            project_name: "test_project".to_string(),
            project_path: temp_dir.clone(),
        };

        let scaffolder = Scaffolder::new(config);
        let result = scaffolder.scaffold();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);

        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_schema_template_contains_project_name() {
        let config = ScaffoldConfig {
            project_name: "my_awesome_project".to_string(),
            project_path: PathBuf::from("my_awesome_project"),
        };

        let scaffolder = Scaffolder::new(config);
        let schema = scaffolder.get_schema_template();

        assert!(schema.contains("my_awesome_project"));
    }

    #[test]
    fn test_gitignore_includes_rust_and_db_entries() {
        let config = ScaffoldConfig {
            project_name: "test".to_string(),
            project_path: PathBuf::from("test"),
        };

        let scaffolder = Scaffolder::new(config);
        let gitignore = scaffolder.get_gitignore_template();

        // Check for Rust entries
        assert!(gitignore.contains("/target"));
        assert!(gitignore.contains("Cargo.lock"));

        // Check for database entries
        assert!(gitignore.contains("/generated"));
        assert!(gitignore.contains("/data"));
        assert!(gitignore.contains("*.db"));
    }

    #[test]
    fn test_config_file_has_valid_toml_structure() {
        let config = ScaffoldConfig {
            project_name: "test_project".to_string(),
            project_path: PathBuf::from("test_project"),
        };

        let scaffolder = Scaffolder::new(config);
        let config_content = scaffolder.get_config_template();

        // Basic validation - should contain key sections
        assert!(config_content.contains("[project]"));
        assert!(config_content.contains("[database]"));
        assert!(config_content.contains("[server]"));
        assert!(config_content.contains("[watch]"));
    }
}
