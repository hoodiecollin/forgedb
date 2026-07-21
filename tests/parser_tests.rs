use forgedb_parser::ast::*;
use forgedb_parser::parser::Parser;

#[test]
fn test_parse_simple_model() {
    let input = r#"
User {
  id: +u64
  email: &string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 1);
    let model = &schema.models[0];
    assert_eq!(model.name, "User");
    assert_eq!(model.fields.len(), 2);

    let id_field = &model.fields[0];
    assert_eq!(id_field.name, "id");
    assert_eq!(id_field.field_type, FieldType::U64);
    assert!(id_field.auto_generate);
    assert!(!id_field.unique);

    let email_field = &model.fields[1];
    assert_eq!(email_field.name, "email");
    assert_eq!(email_field.field_type, FieldType::String);
    assert!(!email_field.auto_generate);
    assert!(email_field.unique);
}

#[test]
fn test_parse_error_empty_model() {
    let input = "User {}";
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
}

#[test]
fn test_parse_error_empty_schema() {
    let input = "";
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
}

#[test]
fn test_parse_field_without_symbols() {
    let input = r#"
User {
  name: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let field = &model.fields[0];
    assert_eq!(field.name, "name");
    assert_eq!(field.field_type, FieldType::String);
    assert!(!field.auto_generate);
    assert!(!field.unique);
}

#[test]
fn test_parse_both_symbols_on_field() {
    let input = r#"
User {
  id: +&u64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let field = &model.fields[0];
    assert_eq!(field.name, "id");
    assert_eq!(field.field_type, FieldType::U64);
    assert!(field.auto_generate);
    assert!(field.unique);
}

#[test]
fn test_parse_symbol_order_reversed() {
    let input = r#"
User {
  id: &+u64
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let field = &model.fields[0];
    assert!(field.auto_generate);
    assert!(field.unique);
}

#[test]
fn test_parse_multiple_unique_fields() {
    let input = r#"
User {
  email: &string
  username: &string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.fields.len(), 2);
    assert!(model.fields[0].unique);
    assert!(model.fields[1].unique);
}

#[test]
fn test_parse_multiple_models() {
    let input = r#"
User {
  id: +u64
  email: &string
}

Post {
  id: +u64
  title: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 2);
    assert_eq!(schema.models[0].name, "User");
    assert_eq!(schema.models[1].name, "Post");
    assert_eq!(schema.models[0].fields.len(), 2);
    assert_eq!(schema.models[1].fields.len(), 2);
}

#[test]
fn test_parse_all_primitive_types() {
    let input = r#"
Model {
  field1: u32
  field2: u64
  field3: i32
  field4: i64
  field5: f64
  field6: bool
  field7: string
  field8: uuid
  field9: timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.fields[0].field_type, FieldType::U32);
    assert_eq!(model.fields[1].field_type, FieldType::U64);
    assert_eq!(model.fields[2].field_type, FieldType::I32);
    assert_eq!(model.fields[3].field_type, FieldType::I64);
    assert_eq!(model.fields[4].field_type, FieldType::F64);
    assert_eq!(model.fields[5].field_type, FieldType::Bool);
    assert_eq!(model.fields[6].field_type, FieldType::String);
    assert_eq!(model.fields[7].field_type, FieldType::Uuid);
    assert_eq!(model.fields[8].field_type, FieldType::Timestamp);
}

#[test]
fn test_parse_duplicate_field_names() {
    let input = r#"
User {
  id: +u64
  email: &string
  email: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Duplicate field name 'email'"));
    assert!(error.contains("model 'User'"));
}

#[test]
fn test_parse_duplicate_model_names() {
    let input = r#"
User {
  id: +u64
}

User {
  email: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Duplicate model name 'User'"));
}

#[test]
fn test_parse_uuid_with_auto_generate() {
    let input = r#"
User {
  id: +uuid
  email: &string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let id_field = &model.fields[0];
    assert_eq!(id_field.field_type, FieldType::Uuid);
    assert!(id_field.auto_generate);
}

#[test]
fn test_parse_timestamp_with_auto_generate() {
    let input = r#"
User {
  created_at: +timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let field = &model.fields[0];
    assert_eq!(field.field_type, FieldType::Timestamp);
    assert!(field.auto_generate);
}

#[test]
fn test_parse_invalid_auto_generate_with_string() {
    let input = r#"
User {
  name: +string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Auto-generate symbol '+' cannot be used"));
}

#[test]
fn test_parse_invalid_auto_generate_with_i32() {
    let input = r#"
User {
  count: +i32
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Auto-generate symbol '+' cannot be used"));
}

#[test]
fn test_parse_invalid_auto_generate_with_bool() {
    let input = r#"
User {
  active: +bool
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Auto-generate symbol '+' cannot be used"));
}

// Validation tests
#[test]
fn test_validation_field_name_snake_case() {
    let input = r#"
User {
  UserName: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("snake_case"));
    assert!(error.contains("user_name"));
}

#[test]
fn test_validation_model_name_pascal_case() {
    let input = r#"
user_model {
  name: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("PascalCase"));
    assert!(error.contains("UserModel"));
}

#[test]
fn test_validation_can_be_disabled() {
    let input = r#"
user_model {
  UserName: string
}
"#;
    let mut parser = Parser::new_with_validation(input, false).unwrap();
    let result = parser.parse();
    // Should succeed when validation is disabled
    assert!(result.is_ok());
}

#[test]
fn test_validation_error_with_line_numbers() {
    let input = r#"
User {
  id: +u64
  BadFieldName: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("line"));
    assert!(error.contains("snake_case"));
}

#[test]
fn test_validation_all_valid() {
    let input = r#"
User {
  id: +u64
  email: &string
  user_name: string
}

Post {
  id: +u64
  title: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_ok());
}

// Integration edge case tests
#[test]
fn test_validation_single_char_names() {
    let input = r#"
A {
  x: u32
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_validation_private_fields() {
    let input = r#"
User {
  id: +u64
  _private: string
  __internal: u32
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_validation_numbers_in_names() {
    let input = r#"
User123 {
  field_123: u32
  abc_456_def: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_validation_mixed_errors_stops_at_first() {
    // Should report the first error encountered (model name)
    let input = r#"
bad_model {
  BadField: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    // Should fail on model name first
    assert!(error.contains("PascalCase"));
    assert!(error.contains("bad_model"));
}

#[test]
fn test_validation_camel_case_field() {
    let input = r#"
User {
  userName: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("snake_case"));
    assert!(error.contains("user_name"));
}

#[test]
fn test_validation_screaming_snake_case_field() {
    let input = r#"
User {
  USER_NAME: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("snake_case"));
}

// Sprint 3: Indexing tests
#[test]
fn test_parse_indexed_field() {
    let input = r#"
User {
  id: +u64
  username: ^string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let username_field = &model.fields[1];
    assert_eq!(username_field.name, "username");
    assert!(username_field.indexed);
    assert!(!username_field.unique);
    assert!(!username_field.auto_generate);
}

#[test]
fn test_parse_indexed_and_unique_field() {
    let input = r#"
User {
  id: +u64
  email: ^&string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let email_field = &model.fields[1];
    assert_eq!(email_field.name, "email");
    assert!(email_field.indexed);
    assert!(email_field.unique);
}

#[test]
fn test_parse_indexed_symbol_order() {
    let input = r#"
User {
  email1: ^&string
  email2: &^string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    // Both orderings should work
    assert!(model.fields[0].indexed);
    assert!(model.fields[0].unique);
    assert!(model.fields[1].indexed);
    assert!(model.fields[1].unique);
}

#[test]
fn test_parse_multiple_indexed_fields() {
    let input = r#"
User {
  id: +uuid
  email: ^&string
  username: ^string
  age: u32
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.fields.len(), 4);

    assert!(model.fields[0].auto_generate);
    assert!(!model.fields[0].indexed);

    assert!(model.fields[1].indexed);
    assert!(model.fields[1].unique);

    assert!(model.fields[2].indexed);
    assert!(!model.fields[2].unique);

    assert!(!model.fields[3].indexed);
    assert!(!model.fields[3].unique);
}

// Sprint 4: Relation tests
#[test]
fn test_parse_one_to_many_relation() {
    let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let posts_field = &model.fields[1];
    assert_eq!(posts_field.name, "posts");
    assert!(posts_field.field_type.is_relation());
    match &posts_field.field_type {
        FieldType::Relation(RelationType::OneToMany(target)) => {
            assert_eq!(target, "Post");
        }
        _ => panic!("Expected OneToMany relation"),
    }
}

#[test]
fn test_parse_required_reference() {
    let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
  author: *User
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[1];
    let author_field = &model.fields[1];
    assert_eq!(author_field.name, "author");
    assert!(author_field.field_type.is_relation());
    match &author_field.field_type {
        FieldType::Relation(RelationType::RequiredReference(target)) => {
            assert_eq!(target, "User");
        }
        _ => panic!("Expected RequiredReference relation"),
    }
}

#[test]
fn test_parse_optional_reference() {
    let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
  reviewer: ?User
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[1];
    let reviewer_field = &model.fields[1];
    assert_eq!(reviewer_field.name, "reviewer");
    assert!(reviewer_field.field_type.is_relation());
    match &reviewer_field.field_type {
        FieldType::Relation(RelationType::OptionalReference(target)) => {
            assert_eq!(target, "User");
        }
        _ => panic!("Expected OptionalReference relation"),
    }
}

#[test]
fn test_parse_full_relation_schema() {
    let input = r#"
User {
  id: +uuid
  email: ^&string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 2);

    let user = &schema.models[0];
    assert_eq!(user.name, "User");
    assert_eq!(user.fields.len(), 3);

    let post = &schema.models[1];
    assert_eq!(post.name, "Post");
    assert_eq!(post.fields.len(), 3);

    // Test relation detection
    let relations = schema.detect_relations();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].parent_model, "User");
    assert_eq!(relations[0].parent_field, "posts");
    assert_eq!(relations[0].child_model, "Post");
    assert_eq!(relations[0].child_field, "author");
    assert!(relations[0].is_required);
}

#[test]
fn test_parse_invalid_relation_undefined_model() {
    let input = r#"
User {
  id: +uuid
  posts: [NonExistentModel]
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("references undefined model"));
    assert!(error.contains("NonExistentModel"));
}

#[test]
fn test_parse_relation_validation() {
    let input = r#"
Post {
  id: +uuid
  author: *User
}

User {
  id: +uuid
  email: string
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    // This should succeed - Post references User which exists
    assert!(result.is_ok());
}

#[test]
fn test_parse_constraint_simple() {
    let input = r#"
User {
  email: string @email
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert_eq!(field.constraints.len(), 1);
    assert_eq!(field.constraints[0].name, "email");
    assert_eq!(field.constraints[0].params.len(), 0);
}

#[test]
fn test_parse_constraint_with_number_param() {
    let input = r#"
User {
  age: u32 @min(0) @max(150)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert_eq!(field.constraints.len(), 2);

    assert_eq!(field.constraints[0].name, "min");
    assert_eq!(field.constraints[0].params.len(), 1);
    assert_eq!(field.constraints[0].params[0], ConstraintParam::Number(0));

    assert_eq!(field.constraints[1].name, "max");
    assert_eq!(field.constraints[1].params.len(), 1);
    assert_eq!(field.constraints[1].params[0], ConstraintParam::Number(150));
}

#[test]
fn test_parse_constraint_multiple() {
    let input = r#"
User {
  password: string @min(8) @private
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert_eq!(field.constraints.len(), 2);
    assert_eq!(field.constraints[0].name, "min");
    assert_eq!(field.constraints[1].name, "private");
}

#[test]
fn test_parse_constraint_with_symbols() {
    let input = r#"
User {
  email: ^&string @email @unique
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert!(field.indexed);
    assert!(field.unique);
    assert_eq!(field.constraints.len(), 2);
    assert_eq!(field.constraints[0].name, "email");
    assert_eq!(field.constraints[1].name, "unique");
}

#[test]
fn test_parse_constraint_complex() {
    let input = r#"
User {
  id: +uuid
  email: ^&string @email
  website: string @url
  age: u32 @min(0) @max(150)
  password: string @min(8) @private
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models[0].fields.len(), 5);

    // id has no constraints
    assert_eq!(schema.models[0].fields[0].constraints.len(), 0);

    // email has @email
    assert_eq!(schema.models[0].fields[1].constraints.len(), 1);
    assert_eq!(schema.models[0].fields[1].constraints[0].name, "email");

    // website has @url
    assert_eq!(schema.models[0].fields[2].constraints.len(), 1);
    assert_eq!(schema.models[0].fields[2].constraints[0].name, "url");

    // age has @min and @max
    assert_eq!(schema.models[0].fields[3].constraints.len(), 2);

    // password has @min and @private
    assert_eq!(schema.models[0].fields[4].constraints.len(), 2);
}

#[test]
fn test_parse_constraint_empty_params() {
    let input = r#"
User {
  email: string @email()
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();

    // Should fail - empty params not allowed for parameterized directive
    assert!(result.is_err());
}

// Sprint 5: Composite Index Tests

#[test]
fn test_parse_composite_index() {
    let input = r#"
User {
  id: +uuid
  first_name: string
  last_name: string

  @index(first_name, last_name)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.composite_indexes.len(), 1);
    assert_eq!(model.composite_indexes[0].fields.len(), 2);
    assert_eq!(model.composite_indexes[0].fields[0], "first_name");
    assert_eq!(model.composite_indexes[0].fields[1], "last_name");
}

#[test]
fn test_parse_multiple_composite_indexes() {
    let input = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  city: string
  state: string

  @index(first_name, last_name)
  @index(city, state)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert_eq!(model.composite_indexes.len(), 2);
    assert_eq!(
        model.composite_indexes[0].fields,
        vec!["first_name", "last_name"]
    );
    assert_eq!(model.composite_indexes[1].fields, vec!["city", "state"]);
}

#[test]
fn test_parse_composite_index_undefined_field() {
    let input = r#"
User {
  id: +uuid
  name: string

  @index(name, email)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("undefined field"));
}

#[test]
fn test_parse_constraint_with_pattern() {
    // Test pattern constraint with identifier (not full regex yet)
    let input = r#"
User {
  phone: string @pattern(phone_regex)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert_eq!(field.constraints.len(), 1);
    assert_eq!(field.constraints[0].name, "pattern");
    assert_eq!(field.constraints[0].params.len(), 1);

    // Check pattern parameter (currently supports identifier, not full regex string)
    match &field.constraints[0].params[0] {
        ConstraintParam::String(s) => {
            assert_eq!(s, "phone_regex");
        }
        _ => panic!("Expected string parameter"),
    }
}

#[test]
fn test_parse_constraint_negative_number() {
    // Test that negative numbers in constraints fail gracefully
    // Current implementation doesn't support negative numbers in lexer
    let input = r#"
Temperature {
  celsius: i32 @min(-273)
}
"#;
    let result = Parser::new(input);

    // Lexer should fail on the '-' character (not a valid token)
    // This test documents the current limitation
    assert!(result.is_err());
    if let Err(e) = result {
        // Should fail during lexing, not parsing
        assert!(e.contains("Unexpected character") || e.contains("Expected"));
    }
}

#[test]
fn test_parse_multiple_constraints_same_type() {
    let input = r#"
User {
  name: string @min(2) @max(50)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let field = &schema.models[0].fields[0];
    assert_eq!(field.constraints.len(), 2);

    // Verify both min and max are present
    assert!(field.constraints.iter().any(|c| c.name == "min"));
    assert!(field.constraints.iter().any(|c| c.name == "max"));
}

#[test]
fn test_parse_btree_index_type_for_ordered_types() {
    let input = r#"
Product {
  id: +uuid
  price: ^f64
  stock: ^u32
  created_at: ^timestamp
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    // Check that ordered types get BTree index type
    assert_eq!(model.fields[1].index_type, IndexType::BTree); // price: f64
    assert_eq!(model.fields[2].index_type, IndexType::BTree); // stock: u32
    assert_eq!(model.fields[3].index_type, IndexType::BTree); // created_at: timestamp
}

#[test]
fn test_parse_hash_index_type_for_unordered_types() {
    let input = r#"
User {
  id: +uuid
  email: ^string
  active: ^bool
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    // Check that unordered types get Hash index type
    assert_eq!(model.fields[1].index_type, IndexType::Hash); // email: string
    assert_eq!(model.fields[2].index_type, IndexType::Hash); // active: bool
}

#[test]
fn test_constraint_helper_methods() {
    let input = r#"
User {
  email: string @email
  age: u32 @min(0) @max(150)
}
"#;
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let email_field = &schema.models[0].fields[0];
    let age_field = &schema.models[0].fields[1];

    // Test has_constraint
    assert!(email_field.has_constraint("email"));
    assert!(!email_field.has_constraint("url"));
    assert!(age_field.has_constraint("min"));
    assert!(age_field.has_constraint("max"));

    // Test get_constraint
    assert!(email_field.get_constraint("email").is_some());
    assert!(email_field.get_constraint("url").is_none());

    let min_constraint = age_field.get_constraint("min").unwrap();
    assert_eq!(min_constraint.params.len(), 1);
}

#[test]
fn test_parse_component_fields() {
    let input = "User {
  id: +uuid
  email: string
  card: tsx://components/user/card
  profile: jsx://components/profile @relations(*)
  verify: api://routes/user/verify
}

Post {
  id: +uuid
  title: string
  author: *User
}";
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 2);
    let user_model = &schema.models[0];
    assert_eq!(user_model.name, "User");
    assert_eq!(user_model.fields.len(), 5);

    // Test TSX component
    let card_field = &user_model.fields[2];
    assert_eq!(card_field.name, "card");
    if let FieldType::Component(comp_ref) = &card_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Tsx);
        assert_eq!(comp_ref.path, "components/user/card");
        assert_eq!(comp_ref.relations, RelationInclusion::None);
    } else {
        panic!("Expected Component field type");
    }

    // Test JSX component with @relations(*)
    let profile_field = &user_model.fields[3];
    assert_eq!(profile_field.name, "profile");
    if let FieldType::Component(comp_ref) = &profile_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Jsx);
        assert_eq!(comp_ref.path, "components/profile");
        assert_eq!(comp_ref.relations, RelationInclusion::All);
    } else {
        panic!("Expected Component field type");
    }

    // Test API component
    let verify_field = &user_model.fields[4];
    assert_eq!(verify_field.name, "verify");
    if let FieldType::Component(comp_ref) = &verify_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Api);
        assert_eq!(comp_ref.path, "routes/user/verify");
    } else {
        panic!("Expected Component field type");
    }
}

#[test]
fn test_parse_component_with_specific_relations() {
    let input = "User {
  id: +uuid
  posts: [Post]
  comments: [Comment]
  card: tsx://components/user/card @relations(posts, comments)
}

Post {
  id: +uuid
  title: string
}

Comment {
  id: +uuid
  text: string
}";
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let user_model = &schema.models[0];
    let card_field = &user_model.fields[3];

    if let FieldType::Component(comp_ref) = &card_field.field_type {
        assert_eq!(comp_ref.protocol, ComponentProtocol::Tsx);
        if let RelationInclusion::Specific(fields) = &comp_ref.relations {
            assert_eq!(fields.len(), 2);
            assert!(fields.contains(&"posts".to_string()));
            assert!(fields.contains(&"comments".to_string()));
        } else {
            panic!("Expected Specific relation inclusion");
        }
    } else {
        panic!("Expected Component field type");
    }
}
