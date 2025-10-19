use forgedb::lexer::*;

#[test]
fn test_basic_tokens() {
    let mut lexer = Lexer::new("+ & : { }");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Plus,
            Token::Ampersand,
            Token::Colon,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_types() {
    let mut lexer = Lexer::new("u32 u64 i32 i64 f64 bool string uuid timestamp");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
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
        ]
    );
}

#[test]
fn test_identifier() {
    let mut lexer = Lexer::new("User email id");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Ident("User".to_string()),
            Token::Ident("email".to_string()),
            Token::Ident("id".to_string()),
            Token::Eof,
        ]
    );
}

#[test]
fn test_multiline_schema() {
    let input = "User\n{\nid\n:\n+ u64\n}"; // Space after + for TypeU64
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
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
        ]
    );
}

#[test]
fn test_consecutive_symbols() {
    let mut lexer = Lexer::new("+& u64"); // Space before u64 for TypeU64
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![Token::Plus, Token::Ampersand, Token::TypeU64, Token::Eof,]
    );
}

#[test]
fn test_mixed_whitespace() {
    let mut lexer = Lexer::new("User\t{\n  id:\t+ u64\r\n}"); // Space after + for TypeU64
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
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
        ]
    );
}

#[test]
fn test_directive_tokens() {
    let mut lexer = Lexer::new("@email @min(10) @max(100)");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::At,
            Token::Ident("email".to_string()),
            Token::At,
            Token::Ident("min".to_string()),
            Token::LParen,
            Token::Number(10),
            Token::RParen,
            Token::At,
            Token::Ident("max".to_string()),
            Token::LParen,
            Token::Number(100),
            Token::RParen,
            Token::Eof,
        ]
    );
}

#[test]
fn test_numeric_literals() {
    let mut lexer = Lexer::new("0 42 1000");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Number(0),
            Token::Number(42),
            Token::Number(1000),
            Token::Eof,
        ]
    );
}

#[test]
fn test_parentheses_and_comma() {
    let mut lexer = Lexer::new("(10, 20)");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LParen,
            Token::Number(10),
            Token::Comma,
            Token::Number(20),
            Token::RParen,
            Token::Eof,
        ]
    );
}

#[test]
fn test_invalid_character() {
    let mut lexer = Lexer::new("User { id: $u64 }");
    let result = lexer.tokenize();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unexpected character '$'"));
}

#[test]
fn test_consecutive_symbols_with_space() {
    let mut lexer = Lexer::new("+ & u64");
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(
        tokens,
        vec![Token::Plus, Token::Ampersand, Token::TypeU64, Token::Eof,]
    );
}
