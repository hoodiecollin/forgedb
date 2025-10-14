use sinkdb::{Parser, CodeGenerator};

fn main() {
    let schema = r#"
User {
    id: +uuid
    email: ^&string
    username: ^string
    age: u32
}
"#;

    let mut parser = Parser::new(schema).unwrap();
    let parsed = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&parsed);

    std::fs::write("generated/database.rs", code).unwrap();
    println!("Generated generated/database.rs");
}
