use forgedb_parser::{Lexer, Parser, Token};

#[test]
fn parses_a_model_with_modifiers_and_directives() {
    let source = r#"
        User {
            id: +uuid
            email: &string @email
            age: i32 @min(18)
        }
    "#;

    let mut parser = Parser::new(source).expect("parse error");
    let schema = parser.parse().expect("parse error");
    assert_eq!(schema.models.len(), 1);
    assert_eq!(schema.models[0].name, "User");
}

#[test]
fn walks_models_and_fields() {
    let source = r#"
        Post {
            id: +uuid
            title: string
            content: string
        }
    "#;

    let mut parser = Parser::new(source).expect("parse error");
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.name, "Post");
    let names: Vec<&str> = model.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["id", "title", "content"]);
}

#[test]
fn tokenizes_a_model() {
    let source = "User { id: +uuid }";
    let mut lexer = Lexer::new(source);

    let tokens: Vec<Token> = lexer.tokenize().expect("lex error");
    assert!(!tokens.is_empty());
}
