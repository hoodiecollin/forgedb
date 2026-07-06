// Hover information for ForgeDB schemas
//
// Provides documentation and type information on hover

use tower_lsp::lsp_types::{Hover, HoverContents, MarkedString, Position};
use crate::parser::{Schema, FieldType};

pub fn get_hover_info(
    content: &str,
    position: Position,
    schema: &Option<Schema>,
) -> Option<Hover> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let word = get_word_at_position(line, position.character as usize)?;

    // Check if it's a type keyword
    if let Some(type_info) = get_type_info(&word) {
        return Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(type_info)),
            range: None,
        });
    }

    // Check if it's a directive
    if word.starts_with('@') || line[..position.character as usize].ends_with('@') {
        let directive_name = word.trim_start_matches('@');
        if let Some(directive_info) = get_directive_info(directive_name) {
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(directive_info)),
                range: None,
            });
        }
    }

    // Check if it's a model or struct reference
    if let Some(schema) = schema {
        if let Some(model) = schema.models.iter().find(|m| m.name == word) {
            let mut info = format!("## Model: {}\n\n", model.name);
            info.push_str("### Fields:\n");
            for field in &model.fields {
                info.push_str(&format!(
                    "- **{}**: {}\n",
                    field.name,
                    format_field_type(&field.field_type)
                ));
            }
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(info)),
                range: None,
            });
        }

        if let Some(s) = schema.structs.iter().find(|s| s.name == word) {
            let mut info = format!("## Struct: {}\n\n", s.name);
            info.push_str("Inline struct type. Fields are stored inline in the parent model.\n\n");
            info.push_str("### Fields:\n");
            for field in &s.fields {
                info.push_str(&format!(
                    "- **{}**: {}\n",
                    field.name,
                    format_field_type(&field.field_type)
                ));
            }
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(info)),
                range: None,
            });
        }
    }

    // Check if it's a modifier symbol
    if let Some(modifier_info) = get_modifier_info(&word) {
        return Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(modifier_info)),
            range: None,
        });
    }

    None
}

fn get_word_at_position(line: &str, char_pos: usize) -> Option<String> {
    if char_pos > line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();
    let mut start = char_pos;
    let mut end = char_pos;

    // Go backwards to find start
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '@') {
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
    match type_name {
        "string" => Some("**string** - Variable-length string\n\nUTF-8 encoded text of any length.".to_string()),
        "bool" => Some("**bool** - Boolean\n\nTrue or false value.".to_string()),
        "u8" => Some("**u8** - Unsigned 8-bit integer\n\nRange: 0 to 255".to_string()),
        "u16" => Some("**u16** - Unsigned 16-bit integer\n\nRange: 0 to 65,535".to_string()),
        "u32" => Some("**u32** - Unsigned 32-bit integer\n\nRange: 0 to 4,294,967,295".to_string()),
        "u64" => Some("**u64** - Unsigned 64-bit integer\n\nRange: 0 to 18,446,744,073,709,551,615".to_string()),
        "i8" => Some("**i8** - Signed 8-bit integer\n\nRange: -128 to 127".to_string()),
        "i16" => Some("**i16** - Signed 16-bit integer\n\nRange: -32,768 to 32,767".to_string()),
        "i32" => Some("**i32** - Signed 32-bit integer\n\nRange: -2,147,483,648 to 2,147,483,647".to_string()),
        "i64" => Some("**i64** - Signed 64-bit integer\n\nRange: -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807".to_string()),
        "f32" => Some("**f32** - 32-bit floating point\n\nSingle-precision IEEE 754 floating point.".to_string()),
        "f64" => Some("**f64** - 64-bit floating point\n\nDouble-precision IEEE 754 floating point.".to_string()),
        "uuid" => Some("**uuid** - Universally Unique Identifier\n\nUUID v4 (randomly generated 128-bit identifier).".to_string()),
        "timestamp" => Some("**timestamp** - Unix timestamp\n\nStored as i64, represents seconds since Unix epoch (January 1, 1970).".to_string()),
        "char" => Some("**char(n)** - Fixed-length string\n\nString with exactly n characters. Example: `char(100)`".to_string()),
        _ => None,
    }
}

fn get_directive_info(directive_name: &str) -> Option<String> {
    match directive_name {
        "email" => Some("**@email** - Email validation\n\nValidates that the field contains a valid email address.\n\n```\nemail: &string @email\n```".to_string()),
        "url" => Some("**@url** - URL validation\n\nValidates that the field contains a valid URL.\n\n```\nwebsite: string? @url\n```".to_string()),
        "min" => Some("**@min(value)** - Minimum value or length\n\nFor numbers: sets minimum value\nFor strings: sets minimum length\n\n```\nage: u32 @min(0)\nusername: string @min(3)\n```".to_string()),
        "max" => Some("**@max(value)** - Maximum value or length\n\nFor numbers: sets maximum value\nFor strings: sets maximum length\n\n```\nage: u32 @max(150)\ntitle: string @max(200)\n```".to_string()),
        "regex" => Some("**@regex(pattern)** - Regular expression validation\n\nValidates field against a regex pattern.\n\n```\nphone: string @regex(\"^\\\\+?[1-9]\\\\d{1,14}$\")\n```".to_string()),
        "length" => Some("**@length(n)** - Exact length\n\nRequires field to be exactly n characters/elements.\n\n```\nzipcode: string @length(5)\n```".to_string()),
        "unique" => Some("**@unique** - Unique constraint\n\nEnsures all values in this field are unique.\n\n```\nemail: ^&string @unique\n```".to_string()),
        "index" => Some("**@index(field1, field2, ...)** - Composite index\n\nCreates an index on one or more fields for faster queries.\n\n```\n@index(user, created_at)\n```".to_string()),
        "fulltext" => Some("**@fulltext** - Full-text search index\n\nEnables full-text search on this field with TF-IDF ranking.\n\n```\ncontent: &string @fulltext\n```".to_string()),
        "computed" => Some("**@computed** - Computed field\n\nMarks field as computed (not stored in database).\nValue is calculated at runtime.\n\n```\nfull_name: string @computed\n```".to_string()),
        "default" => Some("**@default(value)** - Default value\n\nSets a default value if none is provided.\n\n```\nis_active: bool @default(true)\nrole: string @default(\"user\")\n```".to_string()),
        "on_delete" => Some("**@on_delete(action)** - Foreign key behavior\n\nDefines what happens when referenced record is deleted.\n\nActions:\n- `cascade` - Delete this record too\n- `set_null` - Set field to null\n- `restrict` - Prevent deletion\n\n```\nauthor: *User @on_delete(cascade)\n```".to_string()),
        "relations" => Some("**@relations(field1, ...)** - Component relations\n\nSpecifies which relations to include in component props.\nUse `*` to include all relations.\n\n```\ncard: tsx://pages/user/card @relations(posts)\nprofile: tsx://pages/user/profile @relations(*)\n```".to_string()),
        _ => None,
    }
}

fn get_modifier_info(symbol: &str) -> Option<String> {
    match symbol {
        "+" => Some("**+** Auto-generate on create\n\nValue is automatically generated when the record is created (UUID v4 or auto-increment).\n\n```\nid: +uuid\nid: +u64\n```".to_string()),
        "~" => Some("**~** Auto-update on modify\n\nValue is automatically set to the current timestamp whenever the record is updated.\n\n```\nupdated_at: ~timestamp\n```".to_string()),
        "^" => Some("**^** Index\n\nCreates a database index on this field for faster lookups.\n\n```\nusername: ^string\n```".to_string()),
        "&" => Some("**&** Unique\n\nAdds a unique constraint — no two records can share the same value for this field.\n\n```\nemail: ^&string\n```".to_string()),
        // Use backtick escaping: plain `*` to avoid broken Markdown bold/italic.
        "*" => Some("`*` Required foreign-key relation\n\nMarks this field as a required foreign-key reference to another model.\n\n```\nauthor: *User\n```".to_string()),
        "?" => Some("**?** Optional\n\nMarks this field as optional (nullable).\nCan be null/empty.\n\n```\nbio: string?\nage: u32?\n```".to_string()),
        _ => None,
    }
}

fn format_field_type(field_type: &FieldType) -> String {
    match field_type {
        FieldType::String => "string".to_string(),
        FieldType::Bool => "bool".to_string(),
        FieldType::U8 => "u8".to_string(),
        FieldType::U16 => "u16".to_string(),
        FieldType::U32 => "u32".to_string(),
        FieldType::U64 => "u64".to_string(),
        FieldType::I8 => "i8".to_string(),
        FieldType::I16 => "i16".to_string(),
        FieldType::I32 => "i32".to_string(),
        FieldType::I64 => "i64".to_string(),
        FieldType::F32 => "f32".to_string(),
        FieldType::F64 => "f64".to_string(),
        FieldType::Uuid => "uuid".to_string(),
        FieldType::Timestamp => "timestamp".to_string(),
        FieldType::Char(n) => format!("char({})", n),
        FieldType::Model(name) => name.clone(),
        FieldType::Array(inner) => format!("[{}]", format_field_type(inner)),
        FieldType::FixedArray(inner, size) => format!("[{}; {}]", format_field_type(inner), size),
        FieldType::Optional(inner) => format!("{}?", format_field_type(inner)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_schema;

    /// Hovering over a struct name produces hover info that mentions "Struct".
    #[test]
    fn test_hover_struct_name_shows_struct_info() {
        let content = "struct Address {\n  street: string\n}\n\nUser {\n  home: Address\n}\n";
        let schema = parse_schema(content);
        // Position of "Address" on the `home: Address` line (line 5, char 8)
        let position = Position { line: 5, character: 8 };
        let result = get_hover_info(content, position, &schema);
        assert!(result.is_some(), "expected hover info for struct reference");
        if let Some(hover) = result {
            let text = match hover.contents {
                HoverContents::Scalar(MarkedString::String(s)) => s,
                _ => panic!("unexpected hover content type"),
            };
            assert!(
                text.contains("Struct"),
                "hover info should mention 'Struct': {text}"
            );
            assert!(
                text.contains("street"),
                "hover info should list fields: {text}"
            );
        }
    }

    /// Hovering over a model name still works after adding struct support.
    #[test]
    fn test_hover_model_name_still_works() {
        let content = "User {\n  id: +uuid\n}\n\nPost {\n  author: User\n}\n";
        let schema = parse_schema(content);
        // Position of "User" on the `author: User` line (line 5, char 10)
        let position = Position { line: 5, character: 10 };
        let result = get_hover_info(content, position, &schema);
        assert!(result.is_some(), "expected hover info for model reference");
        if let Some(hover) = result {
            let text = match hover.contents {
                HoverContents::Scalar(MarkedString::String(s)) => s,
                _ => panic!("unexpected hover content type"),
            };
            assert!(
                text.contains("Model"),
                "hover info should mention 'Model': {text}"
            );
        }
    }
}
