// Re-export Position from validation crate for consistency
pub use forgedb_validation::Position;

/// Token with position information
#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithPos {
    pub token: Token,
    pub position: Position,
}

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
    TypeJson,    // json - variable-length column typed serde_json::Value
    TypeChar,    // char(N) - fixed-size character array

    // Keywords
    KwStruct, // struct

    // Symbols
    Plus,        // +
    Ampersand,   // &
    Caret,       // ^
    Colon,       // :
    LBrace,      // {
    RBrace,      // }
    LBracket,    // [
    RBracket,    // ]
    Asterisk,    // *
    Question,    // ?
    At,          // @
    LParen,      // (
    RParen,      // )
    Comma,       // ,
    Semicolon,   // ;
    Slash,       // /
    Number(i64), // Numeric literal
    Str(String), // String literal: "..." (directive arguments, e.g. @pattern("^[a-z]+$"))

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

    fn skip_whitespace_with_flag(&mut self) -> bool {
        let start_pos = self.position;
        self.skip_whitespace();
        // Return true if we skipped any whitespace, or if we're at the start of a line
        self.position > start_pos || self.column == 1
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

    fn read_number(&mut self) -> Result<i64, String> {
        let mut num_str = String::new();
        while let Some(ch) = self.current_char() {
            if ch.is_numeric() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        num_str.parse::<i64>().map_err(|e| {
            format!(
                "Numeric literal '{}' is out of range at line {}, column {}: {}",
                num_str, self.line, self.column, e
            )
        })
    }

    /// Read a double-quoted string literal, consuming the opening and closing quotes.
    /// Supports the escapes `\"`, `\\`, `\n`, `\t`, `\r`. Called with the cursor on the
    /// opening `"`.
    fn read_string(&mut self) -> Result<String, String> {
        let start_line = self.line;
        let start_column = self.column;
        self.advance(); // consume opening quote

        let mut value = String::new();
        loop {
            match self.current_char() {
                None => {
                    return Err(format!(
                        "Unterminated string literal starting at line {}, column {}",
                        start_line, start_column
                    ));
                }
                Some('"') => {
                    self.advance(); // consume closing quote
                    return Ok(value);
                }
                Some('\\') => {
                    self.advance();
                    match self.current_char() {
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('r') => value.push('\r'),
                        Some(other) => {
                            return Err(format!(
                                "Invalid escape sequence '\\{}' in string literal at line {}, column {}",
                                other, self.line, self.column
                            ));
                        }
                        None => {
                            return Err(format!(
                                "Unterminated string literal starting at line {}, column {}",
                                start_line, start_column
                            ));
                        }
                    }
                    self.advance();
                }
                Some('\n') => {
                    return Err(format!(
                        "Unterminated string literal starting at line {}, column {} (newline before closing quote)",
                        start_line, start_column
                    ));
                }
                Some(ch) => {
                    value.push(ch);
                    self.advance();
                }
            }
        }
    }

    pub fn next_token(&mut self) -> Result<Token, String> {
        let had_whitespace = self.skip_whitespace_with_flag();

        match self.current_char() {
            None => Ok(Token::Eof),
            Some('\n') => {
                self.advance();
                Ok(Token::Newline)
            }
            Some('/') => {
                self.advance();
                // Only treat // as a comment if there was preceding whitespace or we're at line start
                // This allows tsx://path to work while preserving // comments
                if self.current_char() == Some('/') && had_whitespace {
                    self.advance();
                    self.skip_comment();
                    // After comment, get next token
                    self.next_token()
                } else {
                    // Single slash token (for component paths like tsx://path)
                    Ok(Token::Slash)
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
            Some('^') => {
                self.advance();
                Ok(Token::Caret)
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
            Some('[') => {
                self.advance();
                Ok(Token::LBracket)
            }
            Some(']') => {
                self.advance();
                Ok(Token::RBracket)
            }
            Some('*') => {
                self.advance();
                Ok(Token::Asterisk)
            }
            Some('?') => {
                self.advance();
                Ok(Token::Question)
            }
            Some('@') => {
                self.advance();
                Ok(Token::At)
            }
            Some('(') => {
                self.advance();
                Ok(Token::LParen)
            }
            Some(')') => {
                self.advance();
                Ok(Token::RParen)
            }
            Some(',') => {
                self.advance();
                Ok(Token::Comma)
            }
            Some(';') => {
                self.advance();
                Ok(Token::Semicolon)
            }
            Some('"') => {
                let s = self.read_string()?;
                Ok(Token::Str(s))
            }
            Some(ch) if ch.is_numeric() => {
                let num = self.read_number()?;
                Ok(Token::Number(num))
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
                    "json" => Token::TypeJson,
                    "char" => Token::TypeChar,
                    "struct" => Token::KwStruct,
                    _ => Token::Ident(ident),
                };
                Ok(token)
            }
            Some(ch) => Err(format!(
                "Unexpected character '{}' at line {}, column {}",
                ch, self.line, self.column
            )),
        }
    }

    pub fn next_token_with_pos(&mut self) -> Result<TokenWithPos, String> {
        let pos = Position {
            line: self.line,
            column: self.column,
        };
        let token = self.next_token()?;
        Ok(TokenWithPos {
            token,
            position: pos,
        })
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

    pub fn tokenize_with_pos(&mut self) -> Result<Vec<TokenWithPos>, String> {
        let mut tokens = Vec::new();
        loop {
            let token_with_pos = self.next_token_with_pos()?;
            let is_eof = token_with_pos.token == Token::Eof;
            tokens.push(token_with_pos);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &str) -> Vec<Token> {
        Lexer::new(input).tokenize().expect("lex error")
    }

    #[test]
    fn lexes_basic_string_literal() {
        assert_eq!(
            tokens("\"pending\""),
            vec![Token::Str("pending".to_string()), Token::Eof]
        );
    }

    #[test]
    fn lexes_string_with_regex_metacharacters() {
        // A regex that could never be a bare identifier — the whole point of the feature.
        assert_eq!(
            tokens("\"^[a-z]+$\""),
            vec![Token::Str("^[a-z]+$".to_string()), Token::Eof]
        );
    }

    #[test]
    fn lexes_string_escapes() {
        assert_eq!(
            tokens(r#""a\"b\\c\n\t\r""#),
            vec![Token::Str("a\"b\\c\n\t\r".to_string()), Token::Eof]
        );
    }

    #[test]
    fn empty_string_literal() {
        assert_eq!(tokens("\"\""), vec![Token::Str(String::new()), Token::Eof]);
    }

    #[test]
    fn string_literal_in_directive_context() {
        // @pattern("^[0-9]+$")
        assert_eq!(
            tokens("@pattern(\"^[0-9]+$\")"),
            vec![
                Token::At,
                Token::Ident("pattern".to_string()),
                Token::LParen,
                Token::Str("^[0-9]+$".to_string()),
                Token::RParen,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let err = Lexer::new("\"oops").tokenize().unwrap_err();
        assert!(err.contains("Unterminated string literal"), "got: {err}");
    }

    #[test]
    fn newline_before_closing_quote_is_an_error() {
        let err = Lexer::new("\"oops\n\"").tokenize().unwrap_err();
        assert!(err.contains("Unterminated string literal"), "got: {err}");
    }

    #[test]
    fn invalid_escape_is_an_error() {
        let err = Lexer::new("\"a\\q\"").tokenize().unwrap_err();
        assert!(err.contains("Invalid escape sequence"), "got: {err}");
    }

    #[test]
    fn double_slash_comment_still_works_after_string_support() {
        // Guards the tsx://path vs // comment disambiguation is unaffected.
        assert_eq!(tokens("u32 // trailing comment"), vec![Token::TypeU32, Token::Eof]);
    }
}
