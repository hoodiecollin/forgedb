pub use forgedb_validation::Position;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithPos {
    pub token: Token,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),

    TypeU32,
    TypeU64,
    TypeI32,
    TypeI64,
    TypeF64,
    TypeBool,
    TypeString,
    TypeUuid,
    TypeTimestamp,
    TypeJson,
    TypeDecimal,
    TypeCharDeprecated,

    KwStruct,
    KwEnum,

    Plus,
    Ampersand,
    Caret,
    Colon,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Asterisk,
    Question,
    Bang,
    At,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Slash,
    Gt,
    Lt,
    Number(i64),
    Fractional(String),
    Str(String),

    Newline,
    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    pub line: usize,
    pub column: usize,
    token_start: Position,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
            token_start: Position { line: 1, column: 1 },
        }
    }

    fn current_char(&self) -> Option<char> {
        if self.position < self.input.len() {
            Some(self.input[self.position])
        } else {
            None
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input.get(self.position + 1).copied()
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
        self.position > start_pos || self.column == 1
    }

    fn skip_comment(&mut self) {
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

    fn read_number(&mut self) -> Result<Token, String> {
        let mut lexeme = String::new();
        if self.current_char() == Some('-') {
            lexeme.push('-');
            self.advance();
        }
        while let Some(ch) = self.current_char() {
            if ch.is_ascii_digit() {
                lexeme.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if self.current_char() == Some('.')
            && self.peek_char().is_some_and(|c| c.is_ascii_digit())
        {
            lexeme.push('.');
            self.advance();
            while let Some(ch) = self.current_char() {
                if ch.is_ascii_digit() {
                    lexeme.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
            return Ok(Token::Fractional(lexeme));
        }
        lexeme.parse::<i64>().map(Token::Number).map_err(|e| {
            format!(
                "Numeric literal '{}' is out of range at line {}, column {}: {}",
                lexeme, self.line, self.column, e
            )
        })
    }

    fn read_string(&mut self) -> Result<String, String> {
        let start_line = self.line;
        let start_column = self.column;
        self.advance();

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
                    self.advance();
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
        self.token_start = Position {
            line: self.line,
            column: self.column,
        };

        match self.current_char() {
            None => Ok(Token::Eof),
            Some('\n') => {
                self.advance();
                Ok(Token::Newline)
            }
            Some('/') => {
                self.advance();
                if self.current_char() == Some('/') && had_whitespace {
                    self.advance();
                    self.skip_comment();
                    self.next_token()
                } else {
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
            Some('!') => {
                self.advance();
                Ok(Token::Bang)
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
            Some('>') => {
                self.advance();
                Ok(Token::Gt)
            }
            Some('<') => {
                self.advance();
                Ok(Token::Lt)
            }
            Some('-') if self.peek_char().is_some_and(|c| c.is_ascii_digit()) => self.read_number(),
            Some(ch) if ch.is_ascii_digit() => self.read_number(),
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
                    "decimal" => Token::TypeDecimal,
                    "char" => Token::TypeCharDeprecated,
                    "struct" => Token::KwStruct,
                    "enum" => Token::KwEnum,
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
        let token = self.next_token()?;
        Ok(TokenWithPos {
            token,
            position: self.token_start,
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
        assert_eq!(tokens("u32 // trailing comment"), vec![Token::TypeU32, Token::Eof]);
    }
}
