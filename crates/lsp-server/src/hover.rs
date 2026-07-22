// Hover information for ForgeDB schemas.
//
// Type/directive/modifier docs track the REAL grammar, and model/struct/enum
// hovers render the actual `forgedb_parser` AST.

use forgedb_parser::{FieldType, RelationType, Schema};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkedString, Position};

pub fn get_hover_info(content: &str, position: Position, schema: &Schema) -> Option<Hover> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let word = get_word_at_position(line, position.character as usize)?;

    // Type keyword
    if let Some(type_info) = get_type_info(&word) {
        return Some(scalar_hover(type_info));
    }

    // Directive
    if word.starts_with('@') || line[..position.character as usize].ends_with('@') {
        let directive_name = word.trim_start_matches('@');
        if let Some(directive_info) = get_directive_info(directive_name) {
            return Some(scalar_hover(directive_info));
        }
    }

    // Model / struct / enum reference
    if let Some(model) = schema.models.iter().find(|m| m.name == word) {
        let mut info = format!("## Model: {}\n\n### Fields:\n", model.name);
        for field in &model.fields {
            info.push_str(&format!(
                "- **{}**: {}\n",
                field.name,
                format_field_type(&field.field_type)
            ));
        }
        return Some(scalar_hover(info));
    }

    if let Some(s) = schema.structs.iter().find(|s| s.name == word) {
        let mut info = format!("## Struct: {}\n\n", s.name);
        info.push_str("Inline, fixed-size struct type. Fields are stored inline in the parent model.\n\n### Fields:\n");
        for field in &s.fields {
            info.push_str(&format!(
                "- **{}**: {}\n",
                field.name,
                format_field_type(&field.field_type)
            ));
        }
        return Some(scalar_hover(info));
    }

    if let Some(e) = schema.enums.iter().find(|e| e.name == word) {
        let mut info = format!("## Enum: {}\n\n", e.name);
        info.push_str("Stored as a 1-byte discriminant, serialized as the variant name.\n\n### Variants:\n");
        for variant in &e.variants {
            info.push_str(&format!("- {variant}\n"));
        }
        return Some(scalar_hover(info));
    }

    // Modifier symbol
    if let Some(modifier_info) = get_modifier_info(&word) {
        return Some(scalar_hover(modifier_info));
    }

    None
}

fn scalar_hover(text: String) -> Hover {
    Hover {
        contents: HoverContents::Scalar(MarkedString::String(text)),
        range: None,
    }
}

fn get_word_at_position(line: &str, char_pos: usize) -> Option<String> {
    if char_pos > line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();
    let mut start = char_pos;
    let mut end = char_pos;

    // Go backwards to find start (`@` may lead a directive word)
    while start > 0
        && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '@')
    {
        start -= 1;
    }

    // Go forwards to find end
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

fn get_type_info(type_name: &str) -> Option<String> {
    let s = match type_name {
        "string" => "**string** — Variable-length string\n\nUTF-8 encoded text, stored in a variable-length column.",
        "bool" => "**bool** — Boolean\n\nTrue or false value (1 byte).",
        "u32" => "**u32** — Unsigned 32-bit integer\n\nRange: 0 to 4,294,967,295. Auto-incrementable with `+`.",
        "u64" => "**u64** — Unsigned 64-bit integer\n\nRange: 0 to 18,446,744,073,709,551,615. Auto-incrementable with `+`.",
        "i32" => "**i32** — Signed 32-bit integer\n\nRange: -2,147,483,648 to 2,147,483,647.",
        "i64" => "**i64** — Signed 64-bit integer\n\nRange: -9.2e18 to 9.2e18.",
        "f64" => "**f64** — 64-bit floating point\n\nDouble-precision IEEE 754. Not indexable.",
        "decimal" => "**decimal** — Exact fixed-point decimal\n\n`rust_decimal::Decimal` in a fixed 16-byte column, serialized as a string. Indexable/sortable (scale-invariant key). Money/quantity.",
        "json" => "**json** — Arbitrary JSON value\n\n`serde_json::Value`, stored in a variable-length column. NOT indexable/filterable/sortable (no total order).",
        "uuid" => "**uuid** — UUID v4\n\n128-bit identifier (16-byte column). Auto-generatable with `+`.",
        "timestamp" => "**timestamp** — Unix timestamp\n\nStored as i64 (seconds since epoch). Auto-generatable with `+`.",
        "char" => "**char(n)** — Fixed-length string\n\nExactly n bytes in a fixed column. Example: `char(100)`.",
        _ => return None,
    };
    Some(s.to_string())
}

fn get_directive_info(directive_name: &str) -> Option<String> {
    let s = match directive_name {
        "email" => "**@email** — Email validation\n\n```\nemail: &string @email\n```",
        "url" => "**@url** — URL validation\n\n```\nwebsite: string? @url\n```",
        "min" => "**@min(value)** — Minimum value or length\n\n```\nage: u32 @min(0)\n```",
        "max" => "**@max(value)** — Maximum value or length\n\n```\nage: u32 @max(150)\n```",
        "pattern" | "regex" => "**@pattern(\"…\")** — Regex validation (ENFORCED)\n\nRejects non-matching values at runtime (422). `@regex` is an alias.\n\n```\nphone: string @pattern(\"^\\\\+?[1-9]\\\\d{1,14}$\")\n```",
        "length" => "**@length(n)** — String length\n\n```\nzipcode: string @length(5)\n```",
        "index" => "**@index(a, b, …)** — Composite index (model level)\n\n```\n@index(user, created_at)\n```",
        "on_delete" => "**@on_delete(action)** — Foreign-key on-delete behavior (ENFORCED)\n\n- `restrict` (default) — refuse deleting a referenced parent (409)\n- `cascade` — recursively delete children\n- `set_null` — null the FK (optional FKs only)\n\n```\nauthor: *User @on_delete(cascade)\n```",
        "fulltext" => "**@fulltext** — Full-text marker (semantic-only)\n\nRecorded on the field; no full-text engine is generated today.",
        "computed" => "**@computed** — Computed marker (semantic-only)",
        "materialized" => "**@materialized** — Materialized marker (semantic-only)",
        "default" => "**@default(value)** — Default-value marker (semantic-only)\n\n```\nrole: string @default(\"user\")\n```",
        "soft_delete" => "**@soft_delete** — Soft delete (model level)\n\nRetains rows and filters them out of normal reads.",
        "projection" => "**@projection(name: a, b)** — Named column projection (model level)\n\nGenerates a partial-read struct over PK + the named columns.",
        "relations" => "**@relations(\\*|fields)** — Component relations\n\nSelects which relations a component field includes.\n\n```\ncard: tsx://pages/user/card @relations(posts)\n```",
        _ => return None,
    };
    Some(s.to_string())
}

fn get_modifier_info(symbol: &str) -> Option<String> {
    // ForgeDB has NO `~` auto-update modifier.
    let s = match symbol {
        "+" => "**+** Auto-generate on create\n\nu32/u64 auto-increment, uuid, or timestamp.\n\n```\nid: +uuid\n```",
        "^" => "**^** Index\n\nCreates a database index on this field.\n\n```\nusername: ^string\n```",
        "&" => "**&** Unique\n\nUnique constraint — no two records share this value.\n\n```\nemail: &string\n```",
        "*" => "`*` Required foreign-key relation\n\nRequired FK reference to another model.\n\n```\nauthor: *User\n```",
        "?" => "**?** Optional (nullable)\n\n```\nbio: string?\nmanager: ?User\n```",
        _ => return None,
    };
    Some(s.to_string())
}

fn format_field_type(field_type: &FieldType) -> String {
    match field_type {
        FieldType::String => "string".to_string(),
        FieldType::Bool => "bool".to_string(),
        FieldType::U32 => "u32".to_string(),
        FieldType::U64 => "u64".to_string(),
        FieldType::I32 => "i32".to_string(),
        FieldType::I64 => "i64".to_string(),
        FieldType::F64 => "f64".to_string(),
        FieldType::Decimal => "decimal".to_string(),
        FieldType::Json => "json".to_string(),
        FieldType::Uuid => "uuid".to_string(),
        FieldType::Timestamp => "timestamp".to_string(),
        FieldType::Enum(name) => name.clone(),
        FieldType::Char(n) => format!("char({n})"),
        FieldType::FixedArray(inner, size) => format!("[{}; {}]", format_field_type(inner), size),
        FieldType::StructType(name) => name.clone(),
        FieldType::OptionalStructType(name) => format!("{name}?"),
        FieldType::Nullable(inner) => format!("{}?", format_field_type(inner)),
        FieldType::Relation(rel) => match rel {
            RelationType::OneToMany(m) => format!("[{m}]"),
            RelationType::RequiredReference(m) => format!("*{m}"),
            RelationType::OptionalReference(m) => format!("?{m}"),
            RelationType::ManyToMany(m) => format!("[{m}] (many-to-many)"),
        },
        FieldType::Component(c) => format!("component → {}", c.path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::Parser;

    fn parse(src: &str) -> Schema {
        Parser::new(src).unwrap().parse_recover().schema
    }

    /// Hovering a struct name reports "Struct" and lists its fields.
    #[test]
    fn hover_struct_name_shows_struct_info() {
        let content = "struct Address {\n  street: string\n}\n\nUser {\n  home: Address\n}\n";
        let schema = parse(content);
        let position = Position { line: 5, character: 8 }; // "Address" on the home line
        let text = hover_text(get_hover_info(content, position, &schema));
        assert!(text.contains("Struct"), "got: {text}");
        assert!(text.contains("street"), "got: {text}");
    }

    /// Hovering an enum name reports "Enum" and lists its variants.
    #[test]
    fn hover_enum_name_shows_variants() {
        let content = "enum Status {\n  Active\n  Inactive\n}\n\nUser {\n  status: Status\n}\n";
        let schema = parse(content);
        let position = Position { line: 6, character: 12 }; // "Status" on the status line
        let text = hover_text(get_hover_info(content, position, &schema));
        assert!(text.contains("Enum"), "got: {text}");
        assert!(text.contains("Active"), "got: {text}");
    }

    /// Hovering the `decimal` keyword yields real type documentation.
    #[test]
    fn hover_decimal_type_documented() {
        let content = "User {\n  price: decimal\n}\n";
        let schema = parse(content);
        let position = Position { line: 1, character: 12 };
        let text = hover_text(get_hover_info(content, position, &schema));
        assert!(text.contains("decimal"), "got: {text}");
    }

    fn hover_text(h: Option<Hover>) -> String {
        match h.expect("expected hover").contents {
            HoverContents::Scalar(MarkedString::String(s)) => s,
            _ => panic!("unexpected hover content type"),
        }
    }
}
