// Parser for ForgeDB schema files
//
// Provides AST representation and parsing logic

use regex::Regex;
use tower_lsp::lsp_types::Position;

#[derive(Debug, Clone)]
pub struct Schema {
    pub models: Vec<Model>,
    pub structs: Vec<Struct>,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub modifiers: Vec<FieldModifier>,
    pub directives: Vec<Directive>,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Bool,
    U8, U16, U32, U64,
    I8, I16, I32, I64,
    F32, F64,
    Uuid,
    Timestamp,
    Char(usize),
    Model(String),
    Array(Box<FieldType>),
    FixedArray(Box<FieldType>, usize),
    Optional(Box<FieldType>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldModifier {
    PrimaryKey,  // +
    Required,    // &
    Unique,      // ^
    Relation,    // *
}

#[derive(Debug, Clone)]
pub struct Directive {
    pub name: String,
    pub args: Vec<String>,
}

pub fn parse_schema(content: &str) -> Option<Schema> {
    let mut schema = Schema {
        models: Vec::new(),
        structs: Vec::new(),
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }

        // Parse struct definition
        if line.starts_with("struct ") {
            if let Some((struct_def, lines_consumed)) = parse_struct(&lines[i..], i) {
                schema.structs.push(struct_def);
                i += lines_consumed;
                continue;
            }
        }

        // Parse model definition (starts with uppercase, followed by {)
        if let Some(brace_pos) = line.find('{') {
            let name = line[..brace_pos].trim();
            if !name.is_empty() && name.chars().next().unwrap().is_uppercase() {
                if let Some((model, lines_consumed)) = parse_model(&lines[i..], i) {
                    schema.models.push(model);
                    i += lines_consumed;
                    continue;
                }
            }
        }

        i += 1;
    }

    Some(schema)
}

fn parse_struct(lines: &[&str], start_line: usize) -> Option<(Struct, usize)> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();
    let re = Regex::new(r"^struct\s+([A-Z][a-zA-Z0-9]*)\s*\{").ok()?;
    let caps = re.captures(first_line)?;
    let name = caps.get(1)?.as_str().to_string();

    let mut fields = Vec::new();
    let mut i = 1;

    while i < lines.len() {
        let line = lines[i].trim();

        if line == "}" {
            return Some((Struct {
                name,
                fields,
                position: Position {
                    line: start_line as u32,
                    character: 0,
                },
            }, i + 1));
        }

        if !line.is_empty() && !line.starts_with("//") && !line.starts_with("/*") {
            if let Some(field) = parse_field(line, start_line + i) {
                fields.push(field);
            }
        }

        i += 1;
    }

    None
}

fn parse_model(lines: &[&str], start_line: usize) -> Option<(Model, usize)> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();
    let brace_pos = first_line.find('{')?;
    let name = first_line[..brace_pos].trim().to_string();

    let mut fields = Vec::new();
    let mut i = 1;

    while i < lines.len() {
        let line = lines[i].trim();

        if line == "}" {
            return Some((Model {
                name,
                fields,
                position: Position {
                    line: start_line as u32,
                    character: 0,
                },
            }, i + 1));
        }

        if !line.is_empty() && !line.starts_with("//") && !line.starts_with("/*") {
            if let Some(field) = parse_field(line, start_line + i) {
                fields.push(field);
            }
        }

        i += 1;
    }

    None
}

fn parse_field(line: &str, line_num: usize) -> Option<Field> {
    // Field format: name: [modifiers]type [directives]
    let colon_pos = line.find(':')?;
    let field_name = line[..colon_pos].trim().to_string();
    let rest = line[colon_pos + 1..].trim();

    let (modifiers, field_type, directives) = parse_field_parts(rest)?;

    Some(Field {
        name: field_name,
        field_type,
        modifiers,
        directives,
        position: Position {
            line: line_num as u32,
            character: 0,
        },
    })
}

fn parse_field_parts(s: &str) -> Option<(Vec<FieldModifier>, FieldType, Vec<Directive>)> {
    let mut modifiers = Vec::new();
    let mut rest = s;

    // Parse modifiers (+, &, ^, *)
    while !rest.is_empty() {
        match rest.chars().next()? {
            '+' => {
                modifiers.push(FieldModifier::PrimaryKey);
                rest = &rest[1..].trim_start();
            }
            '&' => {
                modifiers.push(FieldModifier::Required);
                rest = &rest[1..].trim_start();
            }
            '^' => {
                modifiers.push(FieldModifier::Unique);
                rest = &rest[1..].trim_start();
            }
            '*' => {
                modifiers.push(FieldModifier::Relation);
                rest = &rest[1..].trim_start();
            }
            _ => break,
        }
    }

    // Split type from directives
    let parts: Vec<&str> = rest.split('@').collect();
    let type_str = parts[0].trim();

    // Parse field type
    let field_type = parse_field_type(type_str)?;

    // Parse directives
    let mut directives = Vec::new();
    for directive_str in &parts[1..] {
        if let Some(directive) = parse_directive(directive_str.trim()) {
            directives.push(directive);
        }
    }

    Some((modifiers, field_type, directives))
}

fn parse_field_type(s: &str) -> Option<FieldType> {
    let s = s.trim();

    // Check for optional (ends with ?)
    if s.ends_with('?') {
        let inner = parse_field_type(&s[..s.len() - 1])?;
        return Some(FieldType::Optional(Box::new(inner)));
    }

    // Check for array [Type]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];

        // Check for fixed array [type; size]
        if let Some(semi_pos) = inner.find(';') {
            let type_str = inner[..semi_pos].trim();
            let size_str = inner[semi_pos + 1..].trim();
            let size: usize = size_str.parse().ok()?;
            let inner_type = parse_field_type(type_str)?;
            return Some(FieldType::FixedArray(Box::new(inner_type), size));
        }

        // Regular array
        let inner_type = parse_field_type(inner)?;
        return Some(FieldType::Array(Box::new(inner_type)));
    }

    // Check for char(n)
    if s.starts_with("char(") && s.ends_with(')') {
        let size_str = &s[5..s.len() - 1];
        let size: usize = size_str.parse().ok()?;
        return Some(FieldType::Char(size));
    }

    // Primitive types
    match s {
        "string" => Some(FieldType::String),
        "bool" => Some(FieldType::Bool),
        "u8" => Some(FieldType::U8),
        "u16" => Some(FieldType::U16),
        "u32" => Some(FieldType::U32),
        "u64" => Some(FieldType::U64),
        "i8" => Some(FieldType::I8),
        "i16" => Some(FieldType::I16),
        "i32" => Some(FieldType::I32),
        "i64" => Some(FieldType::I64),
        "f32" => Some(FieldType::F32),
        "f64" => Some(FieldType::F64),
        "uuid" => Some(FieldType::Uuid),
        "timestamp" => Some(FieldType::Timestamp),
        _ => {
            // Assume it's a model reference
            if s.chars().next()?.is_uppercase() {
                Some(FieldType::Model(s.to_string()))
            } else {
                None
            }
        }
    }
}

fn parse_directive(s: &str) -> Option<Directive> {
    if let Some(paren_pos) = s.find('(') {
        // Directive with arguments
        let name = s[..paren_pos].trim().to_string();
        let args_str = &s[paren_pos + 1..s.rfind(')')?];
        let args: Vec<String> = args_str.split(',')
            .map(|a| a.trim().to_string())
            .collect();
        Some(Directive { name, args })
    } else {
        // Directive without arguments
        let name = s.split_whitespace().next()?.to_string();
        Some(Directive { name, args: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_model() {
        let schema_text = r#"
User {
  id: +uuid
  email: &string @email
  age: u32?
}
"#;

        let schema = parse_schema(schema_text).unwrap();
        assert_eq!(schema.models.len(), 1);
        assert_eq!(schema.models[0].name, "User");
        assert_eq!(schema.models[0].fields.len(), 3);
    }

    #[test]
    fn test_parse_field_modifiers() {
        let field = parse_field("id: +uuid", 0).unwrap();
        assert!(field.modifiers.contains(&FieldModifier::PrimaryKey));

        let field = parse_field("email: ^&string", 0).unwrap();
        assert!(field.modifiers.contains(&FieldModifier::Unique));
        assert!(field.modifiers.contains(&FieldModifier::Required));
    }

    #[test]
    fn test_parse_array_types() {
        let (_, field_type, _) = parse_field_parts("[Post]").unwrap();
        match field_type {
            FieldType::Array(inner) => {
                assert_eq!(*inner, FieldType::Model("Post".to_string()));
            }
            _ => panic!("Expected array type"),
        }
    }

    #[test]
    fn test_parse_directives() {
        let (_, _, directives) = parse_field_parts("string @min(3) @max(100)").unwrap();
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].name, "min");
        assert_eq!(directives[0].args, vec!["3"]);
    }
}
