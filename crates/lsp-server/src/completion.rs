use forgedb_parser::Schema;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

pub fn get_completions(content: &str, position: Position, schema: &Schema) -> Vec<CompletionItem> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return vec![];
    }

    let current_line = lines[position.line as usize];
    let before_cursor = &current_line[..position.character.min(current_line.len() as u32) as usize];

    if before_cursor.trim_end().ends_with('@') {
        return get_directive_completions();
    }

    if before_cursor.contains(':') && !before_cursor.contains('@') {
        return get_type_completions(schema);
    }

    vec![]
}

fn get_type_completions(schema: &Schema) -> Vec<CompletionItem> {
    let mut completions = vec![];

    let modifiers = [
        ("+", "Auto-generate on create", "Value is generated when the record is created (u32/u64 auto-increment, uuid, or timestamp)"),
        ("&", "Unique", "Adds a unique constraint — no two records can share this value"),
        ("^", "Index", "Creates a database index on this field for faster lookups"),
        ("*", "Required foreign-key relation", "Marks this field as a required foreign-key reference to another model"),
        ("?", "Optional (nullable)", "Marks this field as optional (may be null)"),
    ];
    for (label, detail, doc) in modifiers {
        completions.push(CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some(detail.to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(doc.to_string())),
            ..Default::default()
        });
    }

    let primitive_types = [
        ("string", "Variable-length UTF-8 string"),
        ("bool", "Boolean (true/false)"),
        ("u32", "Unsigned 32-bit integer"),
        ("u64", "Unsigned 64-bit integer"),
        ("i32", "Signed 32-bit integer"),
        ("i64", "Signed 64-bit integer"),
        ("f64", "64-bit floating point"),
        ("decimal", "Exact fixed-point decimal (money/quantity)"),
        ("json", "Arbitrary JSON value (not indexable/filterable)"),
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

    completions.push(CompletionItem {
        label: "bytes(100)".to_string(),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        detail: Some("Fixed-size byte array".to_string()),
        insert_text: Some("bytes($1)".to_string()),
        insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    for model in &schema.models {
        completions.push(CompletionItem {
            label: model.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("Model reference".to_string()),
            ..Default::default()
        });
        completions.push(CompletionItem {
            label: format!("[{}]", model.name),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("One-to-many relation".to_string()),
            ..Default::default()
        });
    }

    for s in &schema.structs {
        let field_summary: String = s
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        completions.push(CompletionItem {
            label: s.name.clone(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("Struct type ({} fields)", s.fields.len())),
            documentation: if field_summary.is_empty() {
                None
            } else {
                Some(tower_lsp::lsp_types::Documentation::String(format!(
                    "Fields: {field_summary}"
                )))
            },
            ..Default::default()
        });
    }

    for e in &schema.enums {
        completions.push(CompletionItem {
            label: e.name.clone(),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some(format!("Enum type ({} variants)", e.variants.len())),
            documentation: if e.variants.is_empty() {
                None
            } else {
                Some(tower_lsp::lsp_types::Documentation::String(format!(
                    "Variants: {}",
                    e.variants.join(", ")
                )))
            },
            ..Default::default()
        });
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
                "Validates that the field contains a valid email address".to_string(),
            )),
            insert_text: Some("email".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@url".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("URL validation".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Validates that the field contains a valid URL".to_string(),
            )),
            insert_text: Some("url".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@min(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Minimum value/length".to_string()),
            insert_text: Some("min($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@max(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Maximum value/length".to_string()),
            insert_text: Some("max($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@pattern(regex)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Regex validation (enforced)".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Rejects values that do not match the regex at runtime (422). `@regex` is an alias.".to_string(),
            )),
            insert_text: Some("pattern($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@length(n)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("String length".to_string()),
            insert_text: Some("length($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@index(fields...)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Composite index (model level)".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Creates a composite index on multiple fields".to_string(),
            )),
            insert_text: Some("index($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@on_delete(action)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Foreign-key on-delete behavior (enforced)".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "restrict (default) | cascade | set_null".to_string(),
            )),
            insert_text: Some("on_delete(${1|restrict,cascade,set_null|})".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@fulltext".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Full-text search marker (semantic-only)".to_string()),
            insert_text: Some("fulltext".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@computed".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Computed field marker (semantic-only)".to_string()),
            insert_text: Some("computed".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@materialized".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Materialized marker (semantic-only)".to_string()),
            insert_text: Some("materialized".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@default(value)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Default value marker (semantic-only)".to_string()),
            insert_text: Some("default($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "@soft_delete".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Soft-delete (model level)".to_string()),
            insert_text: Some("soft_delete".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "@projection(name: fields)".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Named column projection (model level)".to_string()),
            insert_text: Some("projection($1)".to_string()),
            insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::Parser;

    fn parse(src: &str) -> Schema {
        Parser::new(src).unwrap().parse_recover().schema
    }

    #[test]
    fn struct_completions_included_with_kind() {
        let schema = parse("struct Address {\n  street: string\n}\n");
        let content = "User {\n  home: ";
        let position = Position { line: 1, character: 8 };
        let items = get_completions(content, position, &schema);
        let item = items.iter().find(|i| i.label == "Address");
        assert!(item.is_some(), "expected Address struct completion");
        assert_eq!(item.unwrap().kind, Some(CompletionItemKind::STRUCT));
    }

    #[test]
    fn enum_completions_included() {
        let schema = parse("enum Status {\n  Active\n  Inactive\n}\n");
        let content = "User {\n  status: ";
        let position = Position { line: 1, character: 10 };
        let items = get_completions(content, position, &schema);
        let item = items.iter().find(|i| i.label == "Status");
        assert!(item.is_some(), "expected Status enum completion");
        assert_eq!(item.unwrap().kind, Some(CompletionItemKind::ENUM));
    }

    #[test]
    fn type_completions_match_real_grammar() {
        let schema = parse("");
        let content = "User {\n  x: ";
        let position = Position { line: 1, character: 5 };
        let labels: Vec<String> = get_completions(content, position, &schema)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"decimal".to_string()));
        assert!(labels.contains(&"json".to_string()));
        assert!(!labels.contains(&"u8".to_string()), "u8 is not a ForgeDB type");
        assert!(!labels.contains(&"f32".to_string()), "f32 is not a ForgeDB type");
        assert!(!labels.contains(&"~".to_string()), "~ is not a ForgeDB modifier");
    }
}
