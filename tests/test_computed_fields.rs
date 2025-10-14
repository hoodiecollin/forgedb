// Sprint 12: Computed Fields - Comprehensive Tests

use sinkdb::{CodeGenerator, Parser};

#[test]
fn test_parse_computed_directive() {
    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let user_model = schema.find_model("User").expect("User model not found");
    let full_name_field = user_model.fields.iter().find(|f| f.name == "full_name").expect("full_name field not found");

    assert!(full_name_field.is_computed, "full_name should be marked as computed");
    assert!(full_name_field.has_constraint("computed"), "full_name should have @computed constraint");
}

#[test]
fn test_multiple_computed_fields() {
    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
  age: u32
  birth_year: u32 @computed
  posts: [Post]
  post_count: u32 @computed
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let user_model = schema.find_model("User").expect("User model not found");
    let computed_fields: Vec<_> = user_model.fields.iter().filter(|f| f.is_computed).collect();

    assert_eq!(computed_fields.len(), 3, "Should have 3 computed fields");

    let computed_names: Vec<_> = computed_fields.iter().map(|f| f.name.as_str()).collect();
    assert!(computed_names.contains(&"full_name"));
    assert!(computed_names.contains(&"birth_year"));
    assert!(computed_names.contains(&"post_count"));
}

#[test]
fn test_computed_trait_generation() {
    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let codegen = CodeGenerator::new();
    let generated = codegen.generate(&schema);

    // Check trait declaration
    assert!(generated.contains("pub trait UserComputed"), "Should generate UserComputed trait");
    assert!(generated.contains("fn full_name(instance: &User) -> String"), "Should have full_name method");

    // Check default implementation
    assert!(generated.contains("pub struct DefaultUserComputed"), "Should generate default implementation");
    assert!(generated.contains("impl UserComputed for DefaultUserComputed"), "Should implement trait for default");
}

#[test]
fn test_computed_accessor_methods() {
    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
  post_count: u32 @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let codegen = CodeGenerator::new();
    let generated = codegen.generate(&schema);

    // Check for get_with_computed method
    assert!(generated.contains("pub fn get_with_computed<C: UserComputed>"), "Should generate get_with_computed method");

    // Check for compute_* methods for each computed field
    assert!(generated.contains("pub fn compute_full_name<C: UserComputed>"), "Should generate compute_full_name method");
    assert!(generated.contains("pub fn compute_post_count<C: UserComputed>"), "Should generate compute_post_count method");

    // Verify method signatures
    assert!(generated.contains("fn compute_full_name<C: UserComputed>(&self, id: uuid::Uuid) -> Option<String>"),
        "compute_full_name should return Option<String>");
    assert!(generated.contains("fn compute_post_count<C: UserComputed>(&self, id: uuid::Uuid) -> Option<u32>"),
        "compute_post_count should return Option<u32>");
}

#[test]
fn test_computed_fields_in_api_types() {
    let schema_source = r#"
User {
  id: +uuid
  email: string
  full_name: string @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    use sinkdb::ApiCodeGenerator;
    let api_files = ApiCodeGenerator::generate(&schema);

    // Find the types file
    let types_file = api_files.iter()
        .find(|f| f.path.contains("user_types.rs"))
        .expect("Should generate user_types.rs");

    // Check that computed fields are included in Response type
    assert!(types_file.content.contains("pub struct UserResponse"), "Should generate UserResponse struct");
    assert!(types_file.content.contains("// Computed fields"), "Should have computed fields comment");
    assert!(types_file.content.contains("pub full_name: String"), "Response should include full_name computed field");

    // Check that computed fields are NOT in CreateRequest
    // Extract the CreateUserRequest section
    let create_section = types_file.content
        .split("pub struct CreateUserRequest")
        .nth(1)
        .and_then(|s| s.split("}").next());

    if let Some(create_content) = create_section {
        assert!(!create_content.contains("full_name"),
            "CreateRequest should not include computed fields");
    }
}

#[test]
fn test_computed_fields_in_typescript() {
    let schema_source = r#"
User {
  id: +uuid
  email: string
  full_name: string @computed
  score: f64 @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    use sinkdb::TypeScriptGenerator;
    let ts_files = TypeScriptGenerator::generate(&schema);

    // Find the types file
    let types_file = ts_files.iter()
        .find(|f| f.path.contains("types.ts"))
        .expect("Should generate types.ts");

    // Check that computed fields are in the interface
    assert!(types_file.content.contains("export interface User"), "Should generate User interface");
    assert!(types_file.content.contains("// Computed fields"), "Should have computed fields comment");
    assert!(types_file.content.contains("full_name: string"), "Should include full_name as string");
    assert!(types_file.content.contains("score: number"), "Should include score as number");

    // Check that computed fields are NOT in CreateUserRequest
    let create_section = types_file.content
        .split("export interface CreateUserRequest")
        .nth(1)
        .and_then(|s| s.split("}").next());

    if let Some(create_content) = create_section {
        assert!(!create_content.contains("full_name"), "CreateUserRequest should not include computed fields");
        assert!(!create_content.contains("score"), "CreateUserRequest should not include computed fields");
    }
}

#[test]
fn test_computed_field_types() {
    let schema_source = r#"
Model {
  id: +uuid
  count: u32 @computed
  ratio: f64 @computed
  is_active: bool @computed
  description: string @computed
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let model = schema.find_model("Model").expect("Model not found");
    let computed_fields: Vec<_> = model.fields.iter().filter(|f| f.is_computed).collect();

    assert_eq!(computed_fields.len(), 4, "Should have 4 computed fields");

    // Check field types
    use sinkdb::ast::FieldType;
    assert!(matches!(computed_fields[0].field_type, FieldType::U32));
    assert!(matches!(computed_fields[1].field_type, FieldType::F64));
    assert!(matches!(computed_fields[2].field_type, FieldType::Bool));
    assert!(matches!(computed_fields[3].field_type, FieldType::String));
}

#[test]
fn test_no_computed_fields() {
    let schema_source = r#"
User {
  id: +uuid
  email: string
  name: string
}
"#;

    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    let codegen = CodeGenerator::new();
    let generated = codegen.generate(&schema);

    // Should not generate trait if no computed fields
    assert!(!generated.contains("pub trait UserComputed"), "Should not generate trait when no computed fields");
    assert!(!generated.contains("compute_"), "Should not generate compute methods");
}
