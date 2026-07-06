// Code completion for ForgeDB schemas
//
// Provides intelligent autocomplete suggestions

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};
use crate::parser::Schema;

pub fn get_completions(
    content: &str,
    position: Position,
    schema: &Option<Schema>,
) -> Vec<CompletionItem> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return vec![];
    }

    let current_line = lines[position.line as usize];
    let before_cursor = &current_line[..position.character.min(current_line.len() as u32) as usize];

    // Context-aware completions
    if before_cursor.trim_end().ends_with(':') {
        // After colon - suggest field types and modifiers
        return get_type_completions(schema);
    }

    if before_cursor.trim_end().ends_with('@') {
        // After @ - suggest directives
        return get_directive_completions();
    }

    if before_cursor.contains(':') && !before_cursor.contains('@') {
        // In type position - suggest types
        return get_type_completions(schema);
    }

    vec![]
}

fn get_type_completions(schema: &Option<Schema>) -> Vec<CompletionItem> {
    let mut completions = vec![];

    // Modifiers — order matches schema language reference: + ~ ^ & * ?
    completions.push(CompletionItem {
        label: "+".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Auto-generate on create".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Value is automatically generated when the record is created (UUID v4 or auto-increment)".to_string()
        )),
        ..Default::default()
    });

    completions.push(CompletionItem {
        label: "~".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Auto-update on modify".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Value is automatically set to the current timestamp whenever the record is updated".to_string()
        )),
        ..Default::default()
    });

    completions.push(CompletionItem {
        label: "^".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Index".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Creates a database index on this field for faster lookups".to_string()
        )),
        ..Default::default()
    });

    completions.push(CompletionItem {
        label: "&".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Unique".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Adds a unique constraint — no two records can share the same value for this field".to_string()
        )),
        ..Default::default()
    });

    completions.push(CompletionItem {
        label: "*".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Required foreign-key relation".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Marks this field as a required foreign-key reference to another model".to_string()
        )),
        ..Default::default()
    });

    completions.push(CompletionItem {
        label: "?".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("Optional".to_string()),
        documentation: Some(tower_lsp::lsp_types::Documentation::String(
            "Marks this field as optional (nullable)".to_string()
        )),
        ..Default::default()
    });

    // Primitive types
    let primitive_types = [
        ("string", "Variable-length string"),
        ("bool", "Boolean (true/false)"),
        ("u8", "Unsigned 8-bit integer (0-255)"),
        ("u16", "Unsigned 16-bit integer"),
        ("u32", "Unsigned 32-bit integer"),
        ("u64", "Unsigned 64-bit integer"),
        ("i8", "Signed 8-bit integer"),
        ("i16", "Signed 16-bit integer"),
        ("i32", "Signed 32-bit integer"),
        ("i64", "Signed 64-bit integer"),
        ("f32", "32-bit floating point"),
        ("f64", "64-bit floating point"),
        ("uuid", "UUID v4"),
        ("timestamp", "Unix timestamp (i64)"),
    ];

    for (type_name, description) in &primitive_types {
        completions.push(CompletionItem {
            label: type_name.to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some(description.to_string()),
            ..Default::default()
        });
    }

    // char(n) type
    completions.push(CompletionItem {
        label: "char(100)".to_string(),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        detail: Some("Fixed-length string".to_string()),
        insert_text: Some("char($1)".to_string()),
        insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    // Model references (if schema is available)
    if let Some(schema) = schema {
        for model in &schema.models {
            completions.push(CompletionItem {
                label: model.name.clone(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Model reference".to_string()),
                ..Default::default()
            });

            // Array reference
            completions.push(CompletionItem {
                label: format!("[{}]", model.name),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Array of model".to_string()),
                ..Default::default()
            });
        }
    }

    completions
}

fn get_directive_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "@email".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Email validation".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Validates that the field contains a valid email address".to_string()
            )),
            insert_text: Some("email".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@url".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("URL validation".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Validates that the field contains a valid URL".to_string()
            )),
            insert_text: Some("url".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@min(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Minimum value/length".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Sets the minimum value for numbers or minimum length for strings".to_string()
            )),
            insert_text: Some("min($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@max(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Maximum value/length".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Sets the maximum value for numbers or maximum length for strings".to_string()
            )),
            insert_text: Some("max($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@regex(pattern)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Regex validation".to_string()),
            insert_text: Some("regex($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@length(n)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Exact length".to_string()),
            insert_text: Some("length($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@unique".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Unique constraint".to_string()),
            insert_text: Some("unique".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@index(fields...)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Composite index".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Creates a composite index on multiple fields".to_string()
            )),
            insert_text: Some("index($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@fulltext".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Full-text search index".to_string()),
            insert_text: Some("fulltext".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@computed".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Computed field".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Marks this field as computed (not stored in database)".to_string()
            )),
            insert_text: Some("computed".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@default(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Default value".to_string()),
            insert_text: Some("default($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@on_delete(action)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Foreign key on delete behavior".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "cascade | set_null | restrict".to_string()
            )),
            insert_text: Some("on_delete(${1|cascade,set_null,restrict|})".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}
