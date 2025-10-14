use crate::{error::CliError, ui, Result};
use sinkdb::parser::Parser;
use std::fs;

pub struct ValidateOptions {
    pub strict: bool,
    pub schema_only: bool,
    pub implementations: bool,
    pub components: bool,
}

pub fn run(options: ValidateOptions) -> Result<()> {
    ui::header("🔍", "Validating project");

    // Find and read schema file
    let schema_path = find_schema_file()?;
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
                .filter(|f| matches!(f.field_type, sinkdb::ast::FieldType::Relation { .. }))
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
            if let sinkdb::ast::FieldType::Relation(rel_type) = &field.field_type {
                let target = match rel_type {
                    sinkdb::ast::RelationType::OneToMany(t) => t,
                    sinkdb::ast::RelationType::RequiredReference(t) => t,
                    sinkdb::ast::RelationType::OptionalReference(t) => t,
                    sinkdb::ast::RelationType::ManyToMany(t) => t,
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
        let has_timestamp = model.fields.iter().any(|f| {
            matches!(
                f.field_type,
                sinkdb::ast::FieldType::Timestamp
            )
        });

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

    // Determine success/failure
    if !errors.is_empty() {
        println!();
        ui::error(&format!("Validation failed with {} error(s)", errors.len()));

        if options.strict {
            return Err(CliError::SchemaValidation(
                "Schema validation failed".to_string(),
            ));
        }
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
    let candidates = ["schema.sink", "schema.lang", "schema.sinkdb"];

    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(CliError::SchemaNotFound(
        "No schema file found. Expected one of: schema.sink, schema.lang, schema.sinkdb"
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
    s.chars().all(|c| c.is_lowercase() || c == '_' || c.is_numeric())
}
