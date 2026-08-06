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
    TypeDecimal, // decimal - fixed 16-byte column typed rust_decimal::Decimal
    /// The deprecated spelling of `bytes(N)` (#233).
    ///
    /// There is deliberately **no** `TypeBytes` counterpart: `bytes` is a
    /// *contextual* keyword, lexed as an ordinary [`Token::Ident`] and recognized
    /// as the type only in type position followed by `(` (see
    /// `Parser::at_bytes_type`). `char` stays a reserved word because it always
    /// was one. Both spellings produce `FieldType::Bytes(N)` — identical AST,
    /// identical generated Rust (`[u8; N]`), identical column layout and wire form.
    TypeCharDeprecated, // char(N)

    // Keywords
    KwStruct, // struct
    KwEnum,   // enum

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
    Gt,          // >  (exclusive lower bound, `@min(>0)` — #239)
    Lt,          // <  (exclusive upper bound, `@max(<1)` — #239)
    Number(i64), // Integral numeric literal (may be negative — #239)
    /// Fractional numeric literal, carried as its **verbatim source lexeme**
    /// (e.g. `"0.01"`, `"-273.15"`) — #239.
    ///
    /// Deliberately not parsed here. Converting to `f64` at lex time would round
    /// the value before anything knows the target type, which is inherent for an
    /// `f64` field but defeats the entire point of `decimal`. The lexeme is
    /// exact, so codegen can pick the conversion once the field type is known.
    Fractional(String),
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
    /// Start position of the most recently produced token, recorded *after*
    /// leading whitespace/comments are skipped so positions point at the token
    /// itself (not the preceding indentation).
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

    /// The character after the cursor, without consuming anything.
    ///
    /// Needed for the two numeric lookaheads (#239): `-` only starts a number
    /// when a digit follows, and `.` only continues one when a digit follows.
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

    /// Read a numeric literal, returning [`Token::Number`] for an integral one and
    /// [`Token::Fractional`] for one carrying a decimal point.
    ///
    /// Accepts an optional leading `-` (#239 gap 4: `celsius: i32 @min(-273)`
    /// previously failed at the lexer, so a signed field could carry only a
    /// non-negative bound).
    ///
    /// A fractional literal keeps its **verbatim lexeme** rather than being parsed.
    /// The old body built exactly this string and then discarded it on
    /// `parse::<i64>()`; that parse was the single lossy step. Deferring the
    /// conversion to codegen — where the target type is known — is what lets a
    /// `decimal` bound stay exact while an `f64` bound rounds only into its own
    /// domain. See [`Token::Fractional`].
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
        // A '.' belongs to the number only when a digit follows it, so a trailing
        // dot is left for whatever grammar owns it rather than swallowed here.
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
        // Record the token's true start (after skipping indentation/whitespace);
        // comment recursion re-enters here and overwrites this with the real token.
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
            Some('>') => {
                self.advance();
                Ok(Token::Gt)
            }
            Some('<') => {
                self.advance();
                Ok(Token::Lt)
            }
            // `-` starts a number only when a digit follows; otherwise it stays an
            // unexpected character, so a stray dash still fails loudly.
            Some('-') if self.peek_char().is_some_and(|c| c.is_ascii_digit()) => self.read_number(),
            // `is_ascii_digit`, not `is_numeric`: the latter accepts Unicode digits
            // like `٣`, which then fail in `parse::<i64>()` with a confusing
            // "out of range" rather than an unexpected-character error.
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
                    // NOTE: "bytes" is deliberately absent — it is a contextual
                    // keyword and lexes as `Token::Ident` (#233).
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
