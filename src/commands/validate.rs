use crate::{error::CliError, ui, Result};
use forgedb_parser::{ComponentProtocol, FieldType, ParsedSchema, Parser};
use std::fs;
use std::path::Path;

/// The one seam behind CLI↔LSP diagnostic parity (epic #173).
///
/// Both surfaces derive their diagnostics from the *same* resilient parse:
/// `Parser::parse_recover` returns a best-effort AST plus every syntax **and**
/// semantic diagnostic at once (positioned, source-ordered). The CLI renders the
/// resulting `ParsedSchema::diagnostics` here; the LSP (`crates/lsp-server`) calls
/// `parse_recover` inside `update_document` and maps the identical set. The
/// `tests/lsp_cli_parity.rs` fixture pins the two together over `examples/*`, so
/// neither can drift onto a private notion of "valid".
///
/// Lexer errors remain fatal (no token stream to recover from) and surface as the
/// `Err(String)` here — the LSP shows the same message as a single diagnostic.
pub fn parse_and_validate(content: &str) -> std::result::Result<ParsedSchema, String> {
    Ok(Parser::new(content)?.parse_recover())
}

pub struct ValidateOptions {
    pub strict: bool,
    pub schema_only: bool,
    /// `--implementations`: accepted but not yet enforced. The real check —
    /// verifying every `@computed` field resolves — lands with the schema-
    /// expression feature (`@computed(<expr>)`); until then this flag is a
    /// documented no-op. See task #46.
    #[allow(dead_code)]
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

    // Resilient parse via the shared CLI↔LSP seam: one call yields a best-effort
    // AST plus every syntax and semantic diagnostic (positioned, source-ordered) —
    // the exact set the LSP surfaces (#173 parity). Only a lexer error is fatal here.
    let parsed = parse_and_validate(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;
    let schema = &parsed.schema;

    // Syntax + schema-level semantic errors (naming, duplicates, dangling
    // relations/type references, projection/index field references) are ALWAYS
    // fatal — they are not advisory, so they fail regardless of `--strict`.
    //
    // Since #237 this list can also carry `Severity::Warning` entries (schema-
    // language deprecations), so it is partitioned rather than tested for
    // emptiness — the old `!is_empty()` check would have turned every deprecation
    // into a hard validation failure.
    let diag = if parsed.diagnostics.is_empty() {
        crate::diagnostics::Report::default()
    } else {
        println!();
        let counts = crate::diagnostics::report(&parsed.diagnostics);
        println!();
        counts
    };

    if diag.has_errors() {
        ui::error(&format!("Validation failed with {} error(s)", diag.errors));
        return Err(CliError::SchemaValidation(
            "Schema validation failed".to_string(),
        ));
    }

    ui::success("Schema valid");

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

    // Beyond this point the schema is already known semantically valid (the
    // `validate_schema` gate above is fatal). What remains are checks that are
    // NOT pure-schema diagnostics and therefore stay CLI-only:
    //   - filesystem: `--components` verifies referenced component files exist;
    //   - advisory lints: the no-timestamp warning (soft, exit 0). The no-id lint
    //     was promoted to a fatal schema diagnostic in #248.
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

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
    //
    // The missing-identity advisory that used to live here is gone: #248 made it a
    // fatal, positioned diagnostic in `crate::validate::collect_structure_errors`,
    // so it now reaches the LSP too and a schema that would generate uncompilable
    // code no longer exits 0.
    for model in &schema.models {
        // Warn if model has no timestamp fields
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
    } else {
        // Schema-level warnings (#237 deprecations) count toward the summary
        // alongside the CLI-local advisory lints, so the closing line is honest
        // about everything that was printed. Neither kind affects the exit code —
        // not even under `--strict`, which escalates lints, not deprecations.
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
