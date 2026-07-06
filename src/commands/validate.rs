use crate::{error::CliError, ui, Result};
use forgedb_parser::{ComponentProtocol, FieldType, Parser, RelationType};
use std::fs;
use std::path::Path;

pub struct ValidateOptions {
    pub strict: bool,
    pub schema_only: bool,
    pub implementations: bool,
    pub components: bool,
    /// Explicit schema file path (from CLI `--schema` or config `[generate].schema`).
    pub schema: Option<String>,
}

pub fn run(options: ValidateOptions) -> Result<()> {
    ui::header("🔍", "Validating project");

    // Find and read schema file — explicit path takes priority.
    let schema_path = match options.schema.as_deref() {
        Some(p) => p.to_string(),
        None => find_schema_file()?,
    };
    ui::info(&format!("Validating schema: {}", schema_path));

    let schema_content = fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path, e)))?;

    // Parse schema
    let mut parser = Parser::new(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;

    let schema = parser
        .parse()
        .map_err(|e| CliError::SchemaValidation(format!("Parser error: {}", e)))?;

    ui::success("Schema syntax valid");

    // Count statistics
    let model_count = schema.models.len();
    let field_count: usize = schema.models.iter().map(|m| m.fields.len()).sum();
    let relation_count: usize = schema
        .models
        .iter()
        .map(|m| {
            m.fields
                .iter()
                .filter(|f| matches!(f.field_type, FieldType::Relation { .. }))
                .count()
        })
        .sum();

    println!();
    ui::info(&format!("  {} models", model_count));
    ui::info(&format!("  {} fields", field_count));
    ui::info(&format!("  {} relations", relation_count));
    println!();

    // If schema-only mode, we're done
    if options.schema_only {
        ui::success("Validation complete");
        return Ok(());
    }

    // Check for semantic issues
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate model names (PascalCase)
    for model in &schema.models {
        if !is_pascal_case(&model.name) {
            errors.push(format!(
                "Model '{}' should be in PascalCase (e.g., 'User', 'BlogPost')",
                model.name
            ));
        }

        // Validate field names (snake_case)
        for field in &model.fields {
            if !is_snake_case(&field.name) {
                errors.push(format!(
                    "Field '{}.{}' should be in snake_case (e.g., 'user_id', 'created_at')",
                    model.name, field.name
                ));
            }

            // Check for duplicate field names (case-insensitive)
            let duplicate_count = model
                .fields
                .iter()
                .filter(|f| f.name.to_lowercase() == field.name.to_lowercase())
                .count();

            if duplicate_count > 1 {
                errors.push(format!(
                    "Duplicate field name '{}.{}' (field names must be unique)",
                    model.name, field.name
                ));
            }
        }
    }

    // Validate relations reference existing models
    for model in &schema.models {
        for field in &model.fields {
            if let FieldType::Relation(rel_type) = &field.field_type {
                let target = match rel_type {
                    RelationType::OneToMany(t) => t,
                    RelationType::RequiredReference(t) => t,
                    RelationType::OptionalReference(t) => t,
                    RelationType::ManyToMany(t) => t,
                };
                if !schema.models.iter().any(|m| &m.name == target) {
                    errors.push(format!(
                        "Relation '{}.{}' references unknown model '{}'",
                        model.name, field.name, target
                    ));
                }
            }
        }
    }

    // --components: check that component references are well-formed and that
    // tsx:// / jsx:// component files exist on disk relative to the schema directory.
    if options.components {
        let schema_dir = Path::new(&schema_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));

        for model in &schema.models {
            for field in &model.fields {
                if let FieldType::Component(component_ref) = &field.field_type {
                    // Empty path is always an error regardless of protocol.
                    if component_ref.path.is_empty() {
                        errors.push(format!(
                            "Component field '{}.{}' has an empty path",
                            model.name, field.name
                        ));
                        continue;
                    }

                    match component_ref.protocol {
                        ComponentProtocol::Tsx | ComponentProtocol::Jsx => {
                            let ext = match component_ref.protocol {
                                ComponentProtocol::Tsx => "tsx",
                                ComponentProtocol::Jsx => "jsx",
                                ComponentProtocol::Api => unreachable!(),
                            };

                            // Check for the file with its natural extension, or as-is.
                            let with_ext = schema_dir
                                .join(format!("{}.{}", component_ref.path, ext));
                            let as_is = schema_dir.join(&component_ref.path);

                            if !with_ext.exists() && !as_is.exists() {
                                errors.push(format!(
                                    "Component field '{}.{}': referenced file '{}' not found \
                                     (checked '{}' and '{}')",
                                    model.name,
                                    field.name,
                                    component_ref.path,
                                    with_ext.display(),
                                    as_is.display(),
                                ));
                            }
                        }
                        ComponentProtocol::Api => {
                            // api:// paths are logical identifiers, not filesystem paths.
                            // Validate that the path is well-formed (non-empty, no whitespace,
                            // starts with a non-separator character).
                            if component_ref.path.contains(char::is_whitespace) {
                                errors.push(format!(
                                    "Component field '{}.{}': api:// path '{}' must not \
                                     contain whitespace",
                                    model.name, field.name, component_ref.path
                                ));
                            }
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            ui::info("Component references: all OK");
        }
    }

    // Check for potential issues (warnings)
    for model in &schema.models {
        // Warn if model has no ID field
        let has_id = model
            .fields
            .iter()
            .any(|f| f.name == "id" || f.auto_generate);

        if !has_id {
            warnings.push(format!(
                "Model '{}' has no auto-generated ID field (consider adding 'id: +uuid')",
                model.name
            ));
        }

        // Warn if model has no timestamp fields
        let has_timestamp = model
            .fields
            .iter()
            .any(|f| matches!(f.field_type, FieldType::Timestamp));

        if !has_timestamp {
            warnings.push(format!(
                "Model '{}' has no timestamp fields (consider adding 'created_at: +timestamp')",
                model.name
            ));
        }
    }

    // Report errors and warnings
    if !errors.is_empty() {
        println!();
        for error in &errors {
            ui::error(error);
        }
    }

    if !warnings.is_empty() {
        println!();
        for warning in &warnings {
            ui::warning(warning);
        }
    }

    // Determine success/failure.
    //
    // `--strict` is what turns validation issues into a non-zero exit (for CI);
    // plain `validate` is advisory and always exits 0. Keep the message honest so
    // it never says "failed" while the process exits successfully.
    if !errors.is_empty() {
        println!();
        if options.strict {
            ui::error(&format!("Validation failed with {} error(s)", errors.len()));
            return Err(CliError::SchemaValidation(
                "Schema validation failed".to_string(),
            ));
        }
        ui::warning(&format!(
            "Found {} issue(s) — run `validate --strict` to fail on these",
            errors.len()
        ));
    } else if warnings.is_empty() {
        ui::success("Validation passed with no issues");
    } else {
        ui::success(&format!(
            "Validation passed with {} warning(s)",
            warnings.len()
        ));
    }

    Ok(())
}

fn find_schema_file() -> Result<String> {
    let candidates = ["schema.forge", "schema.lang", "schema.forgedb"];

    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(CliError::SchemaNotFound(
        "No schema file found. Expected one of: schema.forge, schema.lang, schema.forgedb"
            .to_string(),
    ))
}

fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // First character should be uppercase
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_uppercase() {
        return false;
    }

    // No underscores or spaces
    if s.contains('_') || s.contains(' ') {
        return false;
    }

    true
}

fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Should be all lowercase with underscores
    s.chars()
        .all(|c| c.is_lowercase() || c == '_' || c.is_numeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_schema(dir: &Path, content: &str) -> String {
        let schema_path = dir.join("schema.forge");
        fs::write(&schema_path, content).unwrap();
        schema_path.to_str().unwrap().to_string()
    }

    /// A schema with a tsx:// component reference to a file that does not exist
    /// should fail validation when `--components --strict` is used.
    #[test]
    fn test_components_tsx_missing_file_strict_fails() {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = write_schema(
            dir.path(),
            r#"
User {
  id: +uuid
  card: tsx://components/UserCard
}
"#,
        );

        let result = run(ValidateOptions {
            strict: true,
            schema_only: false,
            implementations: false,
            components: true,
            schema: Some(schema_path),
        });

        assert!(
            result.is_err(),
            "expected a validation error for missing tsx component file"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("validation"),
            "error should mention validation: {err}"
        );
    }

    /// A schema that has no component references passes `--components` without
    /// errors even when `--strict` is set.
    #[test]
    fn test_components_no_refs_always_passes() {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = write_schema(
            dir.path(),
            r#"
User {
  id: +uuid
  email: string
}
"#,
        );

        let result = run(ValidateOptions {
            strict: true,
            schema_only: false,
            implementations: false,
            components: true,
            schema: Some(schema_path),
        });

        assert!(result.is_ok(), "expected ok for schema with no component refs: {result:?}");
    }

    /// A tsx:// component reference whose file actually exists on disk passes
    /// validation.
    #[test]
    fn test_components_tsx_existing_file_passes() {
        let dir = tempfile::tempdir().unwrap();

        // Create the referenced component file
        let comp_dir = dir.path().join("components");
        fs::create_dir_all(&comp_dir).unwrap();
        fs::write(comp_dir.join("UserCard.tsx"), "export default function UserCard() {}").unwrap();

        let schema_path = write_schema(
            dir.path(),
            r#"
User {
  id: +uuid
  card: tsx://components/UserCard
}
"#,
        );

        let result = run(ValidateOptions {
            strict: true,
            schema_only: false,
            implementations: false,
            components: true,
            schema: Some(schema_path),
        });

        assert!(
            result.is_ok(),
            "expected ok when tsx file exists: {result:?}"
        );
    }
}
