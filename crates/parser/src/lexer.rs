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
    TypeChar, // char(N) - fixed-size character array

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
