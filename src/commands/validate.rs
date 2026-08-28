use crate::{error::CliError, ui, Result};
use forgedb_parser::{ComponentProtocol, FieldType, ParsedSchema, Parser};
use std::fs;
use std::path::Path;

pub fn parse_and_validate(content: &str) -> std::result::Result<ParsedSchema, String> {
    Ok(Parser::new(content)?.parse_recover())
}

pub struct ValidateOptions {
    pub strict: bool,
    pub schema_only: bool,
    #[allow(dead_code)]
    pub implementations: bool,
    pub components: bool,
    pub schema: Option<String>,
}

pub fn run(options: ValidateOptions) -> Result<()> {
    ui::header("🔍", "Validating project");

    let schema_path = match options.schema.as_deref() {
        Some(p) => p.to_string(),
        None => find_schema_file()?,
    };
    ui::info(&format!("Validating schema: {}", schema_path));

    let schema_content = fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path, e)))?;

    let parsed = parse_and_validate(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;
    let schema = &parsed.schema;

    let diag = if parsed.diagnostics.is_empty() {
        crate::diagnostics::Report::default()
    } else {
        ui::blank();
        let counts = crate::diagnostics::report(&parsed.diagnostics);
        ui::blank();
        counts
    };

    if diag.has_errors() {
        ui::error(&format!("Validation failed with {} error(s)", diag.errors));
        return Err(CliError::SchemaValidation(
            "Schema validation failed".to_string(),
        ));
    }

    ui::success("Schema valid");

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

    ui::blank();
    ui::info(&format!("  {} models", model_count));
    ui::info(&format!("  {} fields", field_count));
    ui::info(&format!("  {} relations", relation_count));
    ui::blank();

    if options.schema_only {
        ui::success("Validation complete");
        return Ok(());
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if options.components {
        let schema_dir = crate::project::schema_dir(Path::new(&schema_path));

        for model in &schema.models {
            for field in &model.fields {
                if let FieldType::Component(component_ref) = &field.field_type {
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

    for model in &schema.models {
        let has_timestamp = model
            .fields
            .iter()
            .any(|f| matches!(f.field_type, FieldType::Timestamp(_)));

        if !has_timestamp {
            warnings.push(format!(
                "Model '{}' has no timestamp fields (consider adding 'created_at: +timestamp')",
                model.name
            ));
        }
    }

    if !errors.is_empty() {
        ui::blank();
        for error in &errors {
            ui::error(error);
        }
    }

    if !warnings.is_empty() {
        ui::blank();
        for warning in &warnings {
            ui::warning(warning);
        }
    }

    if !errors.is_empty() {
        ui::blank();
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
    } else {
        let total_warnings = warnings.len() + diag.warnings;
        if total_warnings == 0 {
            ui::success("Validation passed with no issues");
        } else {
            ui::success(&format!(
                "Validation passed with {} warning(s)",
                total_warnings
            ));
        }
    }

    Ok(())
}

fn find_schema_file() -> Result<String> {
    Ok(crate::project::find_schema(None)?.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_schema(dir: &Path, content: &str) -> String {
        let schema_path = dir.join("schema.forge");
        fs::write(&schema_path, content).unwrap();
        schema_path.to_str().unwrap().to_string()
    }

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

    #[test]
    fn test_components_tsx_existing_file_passes() {
        let dir = tempfile::tempdir().unwrap();

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
