// Additional edge case tests for comprehensive coverage

use sinkdb::{ast::FieldType, CodeGenerator, Parser};

// === Type Boundary Tests ===

#[test]
fn test_parse_all_numeric_types_no_autogen() {
    let input = r#"
Model {
  a: i32
  b: i64
  c: f64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    assert_eq!(schema.models[0].fields.len(), 3);
}

#[test]
fn test_parse_invalid_auto_generate_with_f64() {
    let input = r#"
Model {
  value: +f64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("Auto-generate symbol '+' cannot be used"));
}

#[test]
fn test_parse_invalid_auto_generate_with_i64() {
    let input = r#"
Model {
  value: +i64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
}

// === UUID Tests ===

#[test]
fn test_parse_uuid_without_autogen() {
    let input = r#"
Model {
  external_id: uuid
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let field = &schema.models[0].fields[0];
    assert_eq!(field.field_type, FieldType::Uuid);
    assert!(!field.auto_generate);
}

#[test]
fn test_codegen_uuid_without_autogen() {
    let input = r#"
Model {
  id: +u64
  external_id: uuid
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Should have uuid parameter in insert
    assert!(code.contains("external_id: uuid::Uuid"));
    // Should NOT auto-generate
    assert!(!code.contains("let external_id = Uuid::new_v4()"));
}

#[test]
fn test_codegen_uuid_with_unique_constraint() {
    let input = r#"
Model {
  id: +&uuid
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Should have both auto-gen and unique index
    assert!(code.contains("id_index: std::collections::HashMap<uuid::Uuid, usize>"));
    assert!(code.contains("let id = Uuid::new_v4()"));
}

// === Timestamp Tests ===

#[test]
fn test_parse_timestamp_without_autogen() {
    let input = r#"
Model {
  custom_time: timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let field = &schema.models[0].fields[0];
    assert_eq!(field.field_type, FieldType::Timestamp);
    assert!(!field.auto_generate);
}

#[test]
fn test_codegen_timestamp_without_autogen() {
    let input = r#"
Model {
  id: +u64
  custom_time: timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Should have timestamp parameter in insert
    assert!(code.contains("custom_time: i64"));
    // Should NOT auto-generate
    assert!(!code.contains("let custom_time = SystemTime::now()"));
}

#[test]
fn test_codegen_multiple_timestamps() {
    let input = r#"
Model {
  created_at: +timestamp
  updated_at: +timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Both should auto-generate
    assert!(code.contains("let created_at = SystemTime::now()"));
    assert!(code.contains("let updated_at = SystemTime::now()"));
}

// === Boolean Tests ===

#[test]
fn test_codegen_bool_type() {
    let input = r#"
Model {
  id: +u64
  active: bool
  verified: bool
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    assert!(code.contains("pub active: bool"));
    assert!(code.contains("pub verified: bool"));
    assert!(code.contains("active: bool"));
    assert!(code.contains("verified: bool"));
}

#[test]
fn test_parse_bool_with_unique_should_work() {
    // Edge case: bool with unique constraint (unusual but valid)
    let input = r#"
Model {
  id: +u64
  is_primary: &bool
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let field = &schema.models[0].fields[1];
    assert!(field.unique);
    assert_eq!(field.field_type, FieldType::Bool);
}

// === Numeric Type Tests ===

#[test]
fn test_codegen_negative_numeric_types() {
    let input = r#"
Model {
  score: i32
  balance: i64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    assert!(code.contains("pub score: i32"));
    assert!(code.contains("pub balance: i64"));
}

#[test]
fn test_codegen_float_type() {
    let input = r#"
Model {
  price: f64
  rating: f64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    assert!(code.contains("pub price: f64"));
    assert!(code.contains("pub rating: f64"));
}

#[test]
fn test_codegen_unique_numeric_types() {
    // Test unique constraints on different numeric types
    let input = r#"
Model {
  ssn: &u32
  account: &i64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    assert!(code.contains("ssn_index: std::collections::HashMap<u32, usize>"));
    assert!(code.contains("account_index: std::collections::HashMap<i64, usize>"));
}

// === Mixed Type Scenarios ===

#[test]
fn test_codegen_all_types_mixed() {
    let input = r#"
Model {
  id: +uuid
  counter: +u32
  name: string
  score: i32
  amount: i64
  price: f64
  active: bool
  created: +timestamp
  modified: timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Check auto-generated fields are NOT in insert params
    // (they should be in the struct, but not as function parameters)
    let insert_start = code.find("pub fn insert").expect("insert method not found");
    let insert_end = code[insert_start..]
        .find("-> Result")
        .expect("Result not found")
        + insert_start;
    let insert_signature = &code[insert_start..insert_end];

    assert!(
        !insert_signature.contains("id:"),
        "id should not be in insert params"
    );
    assert!(
        !insert_signature.contains("counter:"),
        "counter should not be in insert params"
    );
    assert!(
        !insert_signature.contains("created:"),
        "created should not be in insert params"
    );

    // Check non-auto fields ARE in params
    assert!(code.contains("name: String"));
    assert!(code.contains("score: i32"));
    assert!(code.contains("modified: i64"));

    // Check auto-generation code exists
    assert!(code.contains("let id = Uuid::new_v4()"));
    assert!(code.contains("let counter = self.next_id"));
    assert!(code.contains("let created = SystemTime::now()"));
}

#[test]
fn test_parse_model_with_only_auto_generated_fields() {
    // Edge case: model with no required parameters
    let input = r#"
Model {
  id: +uuid
  created: +timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    assert_eq!(schema.models[0].fields.len(), 2);
    assert!(schema.models[0].fields[0].auto_generate);
    assert!(schema.models[0].fields[1].auto_generate);
}

#[test]
fn test_codegen_model_with_only_auto_generated_fields() {
    let input = r#"
Model {
  id: +uuid
  created: +timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    let generator = CodeGenerator::new();
    let code = generator.generate(&schema);

    // Insert should have no parameters except &mut self
    assert!(code.contains("pub fn insert(&mut self) -> Result<Model, String>"));
}

// === Field Type Helper Tests ===

#[test]
fn test_field_type_is_auto_incrementable() {
    assert!(FieldType::U32.is_auto_incrementable());
    assert!(FieldType::U64.is_auto_incrementable());
    assert!(!FieldType::I32.is_auto_incrementable());
    assert!(!FieldType::Uuid.is_auto_incrementable());
    assert!(!FieldType::Timestamp.is_auto_incrementable());
}

#[test]
fn test_field_type_is_auto_generatable() {
    assert!(FieldType::U32.is_auto_generatable());
    assert!(FieldType::U64.is_auto_generatable());
    assert!(FieldType::Uuid.is_auto_generatable());
    assert!(FieldType::Timestamp.is_auto_generatable());

    assert!(!FieldType::I32.is_auto_generatable());
    assert!(!FieldType::I64.is_auto_generatable());
    assert!(!FieldType::F64.is_auto_generatable());
    assert!(!FieldType::Bool.is_auto_generatable());
    assert!(!FieldType::String.is_auto_generatable());
}

#[test]
fn test_field_type_to_rust_type() {
    assert_eq!(FieldType::I32.to_rust_type(), "i32");
    assert_eq!(FieldType::I64.to_rust_type(), "i64");
    assert_eq!(FieldType::F64.to_rust_type(), "f64");
    assert_eq!(FieldType::Bool.to_rust_type(), "bool");
    assert_eq!(FieldType::Uuid.to_rust_type(), "uuid::Uuid");
    assert_eq!(FieldType::Timestamp.to_rust_type(), "i64");
}
