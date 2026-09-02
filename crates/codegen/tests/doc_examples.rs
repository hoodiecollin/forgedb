use forgedb_codegen::RustGenerator;
use forgedb_parser::Parser;

#[allow(dead_code)]
fn generate_rust_from_a_parsed_schema_compiles() -> Result<(), Box<dyn std::error::Error>> {
    let schema_source = r#"
        User {
            id: +uuid
            email: &string @email
            created_at: timestamp
        }
    "#;

    let mut parser = Parser::new(schema_source).unwrap();
    let schema = parser.parse().unwrap();

    let result = RustGenerator::generate(&schema)?;
    let _ = result.code.lines().count();
    Ok(())
}
