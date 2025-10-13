/// Token types for the schema language
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Identifiers and literals
    Ident(String),

    // Types
    TypeU32,
    TypeU64,
    TypeI32,
    TypeI64,
    TypeF64,
    TypeBool,
    TypeString,
    TypeUuid,
    TypeTimestamp,

    // Symbols
    Plus,        // +
    Ampersand,   // &
    Colon,       // :
    LBrace,      // {
    RBrace,      // }

    // Whitespace and EOF
    Newline,
    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    pub line: usize,
    pub column: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    fn current_char(&self) -> Option<char> {
        if self.position < self.input.len() {
            Some(self.input[self.position])
        } else {
            None
        }
    }

    fn advance(&mut self) {
        if let Some(ch) = self.current_char() {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current_char() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        // Skip until end of line
        while let Some(ch) = self.current_char() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.current_char() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    pub fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace();

        match self.current_char() {
            None => Ok(Token::Eof),
            Some('\n') => {
                self.advance();
                Ok(Token::Newline)
            }
            Some('/') => {
                self.advance();
                if self.current_char() == Some('/') {
                    self.advance();
                    self.skip_comment();
                    // After comment, get next token
                    self.next_token()
                } else {
                    Err(format!("Unexpected character '/' at line {}, column {}", self.line, self.column - 1))
                }
            }
            Some('+') => {
                self.advance();
                Ok(Token::Plus)
            }
            Some('&') => {
                self.advance();
                Ok(Token::Ampersand)
            }
            Some(':') => {
                self.advance();
                Ok(Token::Colon)
            }
            Some('{') => {
                self.advance();
                Ok(Token::LBrace)
            }
            Some('}') => {
                self.advance();
                Ok(Token::RBrace)
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_identifier();
                let token = match ident.as_str() {
                    "u32" => Token::TypeU32,
                    "u64" => Token::TypeU64,
                    "i32" => Token::TypeI32,
                    "i64" => Token::TypeI64,
                    "f64" => Token::TypeF64,
                    "bool" => Token::TypeBool,
                    "string" => Token::TypeString,
                    "uuid" => Token::TypeUuid,
                    "timestamp" => Token::TypeTimestamp,
                    _ => Token::Ident(ident),
                };
                Ok(token)
            }
            Some(ch) => Err(format!("Unexpected character '{}' at line {}, column {}", ch, self.line, self.column))
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("+ & : { }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Plus,
            Token::Ampersand,
            Token::Colon,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_types() {
        let mut lexer = Lexer::new("u32 u64 i32 i64 f64 bool string uuid timestamp");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::TypeU32,
            Token::TypeU64,
            Token::TypeI32,
            Token::TypeI64,
            Token::TypeF64,
            Token::TypeBool,
            Token::TypeString,
            Token::TypeUuid,
            Token::TypeTimestamp,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_identifier() {
        let mut lexer = Lexer::new("User email id");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("User".to_string()),
            Token::Ident("email".to_string()),
            Token::Ident("id".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_multiline_schema() {
        let input = "User\n{\nid\n:\n+ u64\n}";  // Space after + for TypeU64
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("User".to_string()),
            Token::Newline,
            Token::LBrace,
            Token::Newline,
            Token::Ident("id".to_string()),
            Token::Newline,
            Token::Colon,
            Token::Newline,
            Token::Plus,
            Token::TypeU64,
            Token::Newline,
            Token::RBrace,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_consecutive_symbols() {
        let mut lexer = Lexer::new("+& u64");  // Space before u64 for TypeU64
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Plus,
            Token::Ampersand,
            Token::TypeU64,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_mixed_whitespace() {
        let mut lexer = Lexer::new("User\t{\n  id:\t+ u64\r\n}");  // Space after + for TypeU64
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("User".to_string()),
            Token::LBrace,
            Token::Newline,
            Token::Ident("id".to_string()),
            Token::Colon,
            Token::Plus,
            Token::TypeU64,
            Token::Newline,
            Token::RBrace,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_invalid_character() {
        let mut lexer = Lexer::new("User { id: @u64 }");
        let result = lexer.tokenize();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unexpected character '@'"));
    }

    #[test]
    fn test_consecutive_symbols_with_space() {
        let mut lexer = Lexer::new("+ & u64");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens, vec![
            Token::Plus,
            Token::Ampersand,
            Token::TypeU64,
            Token::Eof,
        ]);
    }
}
