// Diagnostics for ForgeDB schema validation
//
// Provides real-time error checking and validation

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use crate::parser::{Schema, Field, FieldType, FieldModifier};

pub fn validate_schema(schema: &Schema, _content: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Validate models
    for model in &schema.models {
        // Check for duplicate field names
        let mut field_names = std::collections::HashSet::new();
        for field in &model.fields {
            if !field_names.insert(&field.name) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: field.position,
                        end: Position {
                            line: field.position.line,
                            character: field.position.character + field.name.len() as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Duplicate field name '{}'", field.name),
                    ..Default::default()
                });
            }

            // Validate field
            validate_field(field, &mut diagnostics);
        }

        // Check for auto-generated primary key field (+)
        let has_primary_key = model.fields.iter().any(|f| {
            f.modifiers.contains(&FieldModifier::AutoGenerate)
        });

        if !has_primary_key {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: model.position,
                    end: Position {
                        line: model.position.line,
                        character: model.position.character + model.name.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!("Model '{}' should have a primary key field", model.name),
                ..Default::default()
            });
        }
    }

    // Check for undefined model references
    let model_names: std::collections::HashSet<_> = schema.models.iter()
        .map(|m| &m.name)
        .collect();

    for model in &schema.models {
        for field in &model.fields {
            check_model_references(&field.field_type, &model_names, field, &mut diagnostics);
        }
    }

    diagnostics
}

fn validate_field(field: &Field, diagnostics: &mut Vec<Diagnostic>) {
    // Check for invalid modifier combinations
    if field.modifiers.contains(&FieldModifier::AutoGenerate) {
        // Auto-generated fields should not be optional
        if is_optional(&field.field_type) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: field.position,
                    end: Position {
                        line: field.position.line,
                        character: field.position.character + field.name.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Auto-generated field (+) cannot be optional".to_string(),
                ..Default::default()
            });
        }
    }

    // Validate directives
    for directive in &field.directives {
        match directive.name.as_str() {
            "min" | "max" => {
                // Should have exactly one argument
                if directive.args.len() != 1 {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: field.position,
                            end: Position {
                                line: field.position.line,
                                character: field.position.character + 50,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("@{} directive requires exactly one argument", directive.name),
                        ..Default::default()
                    });
                }

                // Argument should be a number for numeric types
                if let Some(arg) = directive.args.first() {
                    if arg.parse::<i64>().is_err() && arg.parse::<f64>().is_err() {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: field.position,
                                end: Position {
                                    line: field.position.line,
                                    character: field.position.character + 50,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!("@{} directive argument must be a number", directive.name),
                            ..Default::default()
                        });
                    }
                }
            }
            "email" | "url" => {
                // Should be applied to string fields
                if !matches!(unwrap_type(&field.field_type), FieldType::String) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: field.position,
                            end: Position {
                                line: field.position.line,
                                character: field.position.character + 50,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("@{} directive is typically used with string fields", directive.name),
                        ..Default::default()
                    });
                }
            }
            "computed" => {
                // Computed fields shouldn't have modifiers
                if !field.modifiers.is_empty() {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: field.position,
                            end: Position {
                                line: field.position.line,
                                character: field.position.character + 50,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: "@computed fields should not have modifiers".to_string(),
                        ..Default::default()
                    });
                }
            }
            "on_delete" => {
                // Should be on required FK fields (*)
                if !field.modifiers.contains(&FieldModifier::RequiredFk) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: field.position,
                            end: Position {
                                line: field.position.line,
                                character: field.position.character + 50,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: "@on_delete should be used on relation fields".to_string(),
                        ..Default::default()
                    });
                }

                // Validate argument
                if let Some(arg) = directive.args.first() {
                    if !["cascade", "set_null", "restrict"].contains(&arg.as_str()) {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: field.position,
                                end: Position {
                                    line: field.position.line,
                                    character: field.position.character + 50,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!("Invalid on_delete action: {}. Must be cascade, set_null, or restrict", arg),
                            ..Default::default()
                        });
                    }
                }
            }
            _ => {
                // Unknown directive - warning
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: field.position,
                        end: Position {
                            line: field.position.line,
                            character: field.position.character + 50,
                        },
                    },
                    severity: Some(DiagnosticSeverity::INFORMATION),
                    message: format!("Unknown directive: @{}", directive.name),
                    ..Default::default()
                });
            }
        }
    }
}

fn is_optional(field_type: &FieldType) -> bool {
    matches!(field_type, FieldType::Optional(_))
}

fn unwrap_type(field_type: &FieldType) -> &FieldType {
    match field_type {
        FieldType::Optional(inner) => unwrap_type(inner),
        FieldType::Array(inner) => unwrap_type(inner),
        _ => field_type,
    }
}

fn check_model_references(
    field_type: &FieldType,
    model_names: &std::collections::HashSet<&String>,
    field: &Field,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match field_type {
        FieldType::Model(name) => {
            if !model_names.contains(name) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: field.position,
                        end: Position {
                            line: field.position.line,
                            character: field.position.character + 50,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Undefined model reference: '{}'", name),
                    ..Default::default()
                });
            }
        }
        FieldType::Array(inner) | FieldType::Optional(inner) => {
            check_model_references(inner, model_names, field, diagnostics);
        }
        FieldType::FixedArray(inner, _) => {
            check_model_references(inner, model_names, field, diagnostics);
        }
        _ => {}
    }
}
