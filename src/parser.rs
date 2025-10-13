use crate::ast::{Field, FieldType, Model, Schema};
use crate::lexer::{Lexer, Token};

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(input: &str) -> Result<Self, String> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;
        Ok(Parser { tokens, position: 0 })
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
            Token::TypeString => FieldType::String,
            _ => return Err(format!("Expected type, found {:?}", self.current_token())),
        };
        self.advance();
        Ok(field_type)
    }

    fn parse_field(&mut self) -> Result<Field, String> {
        self.skip_newlines();

        // Parse field name
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => return Err(format!("Expected field name, found {:?}", self.current_token())),
        };
        self.advance();

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
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => return Err(format!("Expected model name, found {:?}", self.current_token())),
        };
        self.advance();

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
  field3: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let model = &schema.models[0];
        assert_eq!(model.fields[0].field_type, FieldType::U32);
        assert_eq!(model.fields[1].field_type, FieldType::U64);
        assert_eq!(model.fields[2].field_type, FieldType::String);
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
}
