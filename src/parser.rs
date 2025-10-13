use crate::ast::{Field, FieldType, Model, Schema};
use crate::lexer::{Lexer, Token, TokenWithPos};
use sinkdb_validation::{validate_field_name, validate_model_name, Position};

pub struct Parser {
    tokens: Vec<Token>,
    tokens_with_pos: Vec<TokenWithPos>,
    position: usize,
    use_validation: bool,
}

impl Parser {
    pub fn new(input: &str) -> Result<Self, String> {
        Self::new_with_validation(input, true)
    }

    pub fn new_with_validation(input: &str, use_validation: bool) -> Result<Self, String> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;

        let mut lexer = Lexer::new(input);
        let tokens_with_pos = lexer.tokenize_with_pos()?;

        Ok(Parser {
            tokens,
            tokens_with_pos,
            position: 0,
            use_validation,
        })
    }

    fn get_current_position(&self) -> Option<Position> {
        if self.position < self.tokens_with_pos.len() {
            Some(self.tokens_with_pos[self.position].position)
        } else {
            None
        }
    }

    fn current_token(&self) -> &Token {
        if self.position < self.tokens.len() {
            &self.tokens[self.position]
        } else {
            &Token::Eof
        }
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current_token(), Token::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        if self.current_token() == &expected {
            self.advance();
            Ok(())
        } else {
            Err(format!("Expected {:?}, found {:?}", expected, self.current_token()))
        }
    }

    fn parse_type(&mut self) -> Result<FieldType, String> {
        let field_type = match self.current_token() {
            Token::TypeU32 => FieldType::U32,
            Token::TypeU64 => FieldType::U64,
            Token::TypeI32 => FieldType::I32,
            Token::TypeI64 => FieldType::I64,
            Token::TypeF64 => FieldType::F64,
            Token::TypeBool => FieldType::Bool,
            Token::TypeString => FieldType::String,
            Token::TypeUuid => FieldType::Uuid,
            Token::TypeTimestamp => FieldType::Timestamp,
            _ => return Err(format!("Expected type, found {:?}", self.current_token())),
        };
        self.advance();
        Ok(field_type)
    }

    fn parse_field(&mut self) -> Result<Field, String> {
        self.skip_newlines();

        // Parse field name
        let field_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => return Err(format!("Expected field name, found {:?}", self.current_token())),
        };
        self.advance();

        // Validate field name
        if self.use_validation {
            if let Err(e) = validate_field_name(&name, field_pos) {
                return Err(e.to_string());
            }
        }

        // Expect colon
        self.expect(Token::Colon)?;

        // Check for symbols (+ and &)
        let mut auto_generate = false;
        let mut unique = false;

        loop {
            match self.current_token() {
                Token::Plus => {
                    auto_generate = true;
                    self.advance();
                }
                Token::Ampersand => {
                    unique = true;
                    self.advance();
                }
                _ => break,
            }
        }

        // Parse type
        let field_type = self.parse_type()?;

        // Validate auto-generate is compatible with type
        if auto_generate && !field_type.is_auto_generatable() {
            return Err(format!(
                "Auto-generate symbol '+' cannot be used with type {:?}. Only u32, u64, uuid, and timestamp support auto-generation",
                field_type
            ));
        }

        Ok(Field {
            name,
            field_type,
            auto_generate,
            unique,
        })
    }

    fn parse_model(&mut self) -> Result<Model, String> {
        self.skip_newlines();

        // Parse model name
        let model_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => return Err(format!("Expected model name, found {:?}", self.current_token())),
        };
        self.advance();

        // Validate model name
        if self.use_validation {
            if let Err(e) = validate_model_name(&name, model_pos) {
                return Err(e.to_string());
            }
        }

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;

        // Parse fields
        let mut fields = Vec::new();
        let mut field_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field()?;

            // Check for duplicate field name
            if field_names.contains(&field.name) {
                return Err(format!("Duplicate field name '{}' in model '{}'", field.name, name));
            }
            field_names.insert(field.name.clone());

            fields.push(field);
            self.skip_newlines();
        }

        // Expect closing brace
        self.expect(Token::RBrace)?;

        if fields.is_empty() {
            return Err(format!("Model '{}' has no fields", name));
        }

        Ok(Model { name, fields })
    }

    pub fn parse(&mut self) -> Result<Schema, String> {
        let mut models = Vec::new();
        let mut model_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::Eof) {
            let model = self.parse_model()?;

            // Check for duplicate model name
            if model_names.contains(&model.name) {
                return Err(format!("Duplicate model name '{}'", model.name));
            }
            model_names.insert(model.name.clone());

            models.push(model);
            self.skip_newlines();
        }

        if models.is_empty() {
            return Err("Schema is empty".to_string());
        }

        Ok(Schema { models })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_model() {
        let input = r#"
User {
  id: +u64
  email: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        assert_eq!(schema.models.len(), 1);
        let model = &schema.models[0];
        assert_eq!(model.name, "User");
        assert_eq!(model.fields.len(), 2);

        let id_field = &model.fields[0];
        assert_eq!(id_field.name, "id");
        assert_eq!(id_field.field_type, FieldType::U64);
        assert!(id_field.auto_generate);
        assert!(!id_field.unique);

        let email_field = &model.fields[1];
        assert_eq!(email_field.name, "email");
        assert_eq!(email_field.field_type, FieldType::String);
        assert!(!email_field.auto_generate);
        assert!(email_field.unique);
    }

    #[test]
    fn test_parse_error_empty_model() {
        let input = "User {}";
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_empty_schema() {
        let input = "";
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_field_without_symbols() {
        let input = r#"
User {
  name: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        let field = &model.fields[0];
        assert_eq!(field.name, "name");
        assert_eq!(field.field_type, FieldType::String);
        assert!(!field.auto_generate);
        assert!(!field.unique);
    }

    #[test]
    fn test_parse_both_symbols_on_field() {
        let input = r#"
User {
  id: +&u64
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        let field = &model.fields[0];
        assert_eq!(field.name, "id");
        assert_eq!(field.field_type, FieldType::U64);
        assert!(field.auto_generate);
        assert!(field.unique);
    }

    #[test]
    fn test_parse_symbol_order_reversed() {
        let input = r#"
User {
  id: &+u64
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        let field = &model.fields[0];
        assert!(field.auto_generate);
        assert!(field.unique);
    }

    #[test]
    fn test_parse_multiple_unique_fields() {
        let input = r#"
User {
  email: &string
  username: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        assert_eq!(model.fields.len(), 2);
        assert!(model.fields[0].unique);
        assert!(model.fields[1].unique);
    }

    #[test]
    fn test_parse_multiple_models() {
        let input = r#"
User {
  id: +u64
  email: &string
}

Post {
  id: +u64
  title: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        assert_eq!(schema.models.len(), 2);
        assert_eq!(schema.models[0].name, "User");
        assert_eq!(schema.models[1].name, "Post");
        assert_eq!(schema.models[0].fields.len(), 2);
        assert_eq!(schema.models[1].fields.len(), 2);
    }

    #[test]
    fn test_parse_all_primitive_types() {
        let input = r#"
Model {
  field1: u32
  field2: u64
  field3: i32
  field4: i64
  field5: f64
  field6: bool
  field7: string
  field8: uuid
  field9: timestamp
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        assert_eq!(model.fields[0].field_type, FieldType::U32);
        assert_eq!(model.fields[1].field_type, FieldType::U64);
        assert_eq!(model.fields[2].field_type, FieldType::I32);
        assert_eq!(model.fields[3].field_type, FieldType::I64);
        assert_eq!(model.fields[4].field_type, FieldType::F64);
        assert_eq!(model.fields[5].field_type, FieldType::Bool);
        assert_eq!(model.fields[6].field_type, FieldType::String);
        assert_eq!(model.fields[7].field_type, FieldType::Uuid);
        assert_eq!(model.fields[8].field_type, FieldType::Timestamp);
    }

    #[test]
    fn test_parse_duplicate_field_names() {
        let input = r#"
User {
  id: +u64
  email: &string
  email: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("Duplicate field name 'email'"));
        assert!(error.contains("model 'User'"));
    }

    #[test]
    fn test_parse_duplicate_model_names() {
        let input = r#"
User {
  id: +u64
}

User {
  email: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("Duplicate model name 'User'"));
    }

    #[test]
    fn test_parse_uuid_with_auto_generate() {
        let input = r#"
User {
  id: +uuid
  email: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        let id_field = &model.fields[0];
        assert_eq!(id_field.field_type, FieldType::Uuid);
        assert!(id_field.auto_generate);
    }

    #[test]
    fn test_parse_timestamp_with_auto_generate() {
        let input = r#"
User {
  created_at: +timestamp
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        let field = &model.fields[0];
        assert_eq!(field.field_type, FieldType::Timestamp);
        assert!(field.auto_generate);
    }

    #[test]
    fn test_parse_invalid_auto_generate_with_string() {
        let input = r#"
User {
  name: +string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("Auto-generate symbol '+' cannot be used"));
    }

    #[test]
    fn test_parse_invalid_auto_generate_with_i32() {
        let input = r#"
User {
  count: +i32
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("Auto-generate symbol '+' cannot be used"));
    }

    #[test]
    fn test_parse_invalid_auto_generate_with_bool() {
        let input = r#"
User {
  active: +bool
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("Auto-generate symbol '+' cannot be used"));
    }

    // Validation tests
    #[test]
    fn test_validation_field_name_snake_case() {
        let input = r#"
User {
  UserName: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("snake_case"));
        assert!(error.contains("user_name"));
    }

    #[test]
    fn test_validation_model_name_pascal_case() {
        let input = r#"
user_model {
  name: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("PascalCase"));
        assert!(error.contains("UserModel"));
    }

    #[test]
    fn test_validation_can_be_disabled() {
        let input = r#"
user_model {
  UserName: string
}
"#;
        let mut parser = Parser::new_with_validation(input, false).unwrap();
        let result = parser.parse();
        // Should succeed when validation is disabled
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_error_with_line_numbers() {
        let input = r#"
User {
  id: +u64
  BadFieldName: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("line"));
        assert!(error.contains("snake_case"));
    }

    #[test]
    fn test_validation_all_valid() {
        let input = r#"
User {
  id: +u64
  email: &string
  user_name: string
}

Post {
  id: +u64
  title: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_ok());
    }

    // Integration edge case tests
    #[test]
    fn test_validation_single_char_names() {
        let input = r#"
A {
  x: u32
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_private_fields() {
        let input = r#"
User {
  id: +u64
  _private: string
  __internal: u32
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_numbers_in_names() {
        let input = r#"
User123 {
  field_123: u32
  abc_456_def: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_mixed_errors_stops_at_first() {
        // Should report the first error encountered (model name)
        let input = r#"
bad_model {
  BadField: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Should fail on model name first
        assert!(error.contains("PascalCase"));
        assert!(error.contains("bad_model"));
    }

    #[test]
    fn test_validation_camel_case_field() {
        let input = r#"
User {
  userName: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("snake_case"));
        assert!(error.contains("user_name"));
    }

    #[test]
    fn test_validation_screaming_snake_case_field() {
        let input = r#"
User {
  USER_NAME: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("snake_case"));
    }
}
