/// Advanced Features Tests
///
/// Tests for:
/// - Materialized computed fields
/// - Soft delete
/// - Batch operations
/// - Partial field selection (schema parsing)
use sinkdb::{ast::FieldType, CodeGenerator, Parser};

#[test]
fn test_parse_materialized_directive() {
    let input = r#"
User {
id: +uuid
first_name: string
last_name: string
full_name: string @computed @materialized
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 1);
    let model = &schema.models[0];
    assert_eq!(model.name, "User");

    // Find the full_name field
    let full_name_field = model.fields.iter().find(|f| f.name == "full_name").unwrap();

    assert!(full_name_field.is_computed, "Should be computed");
    assert!(full_name_field.is_materialized, "Should be materialized");
}

#[test]
fn test_parse_soft_delete_directive() {
    let input = r#"
User {
id: +uuid
email: &string
@soft_delete
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 1);
    let model = &schema.models[0];
    assert_eq!(model.name, "User");
    assert!(model.soft_delete, "Model should have soft_delete enabled");
}

#[test]
fn test_parse_soft_delete_with_fields() {
    let input = r#"
Product {
id: +uuid
name: string
category: string
@soft_delete
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    assert_eq!(schema.models.len(), 1);
    let model = &schema.models[0];
    assert_eq!(model.name, "Product");
    assert!(model.soft_delete, "Model should have soft_delete enabled");
    assert_eq!(model.fields.len(), 3);
}

#[test]
fn test_materialized_without_computed_fails() {
    // @materialized should only be used with @computed
    let input = r#"
User {
id: +uuid
cached_value: string @materialized
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    // The parser allows this, but we should validate in codegen
    let model = &schema.models[0];
    let field = model
        .fields
        .iter()
        .find(|f| f.name == "cached_value")
        .unwrap();

    // Field should have is_materialized but not is_computed
    assert!(field.is_materialized);
    assert!(!field.is_computed);
}

#[test]
fn test_computed_with_materialized() {
    let input = r#"
Order {
id: +uuid
quantity: u32
price: f64
total: f64 @computed @materialized
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let total_field = model.fields.iter().find(|f| f.name == "total").unwrap();

    assert!(total_field.is_computed);
    assert!(total_field.is_materialized);
    assert_eq!(total_field.field_type, FieldType::F64);
}

#[test]
fn test_codegen_generates_materialized_storage() {
    let input = r#"
Product {
id: +uuid
name: string
price: f64
quantity: u32
total_value: f64 @computed @materialized
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Check that materialized storage is generated
    assert!(
        code.contains("materialized_total_value"),
        "Should generate materialized storage for total_value"
    );
    assert!(
        code.contains("get_materialized_total_value"),
        "Should generate getter for materialized field"
    );
    assert!(
        code.contains("recompute_materialized_total_value"),
        "Should generate recompute method for materialized field"
    );
}

#[test]
fn test_codegen_generates_soft_delete() {
    let input = r#"
User {
id: +uuid
email: &string
@soft_delete
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Check for soft delete features
    assert!(code.contains("deleted_at"), "Should have deleted_at field");
    assert!(code.contains("fn restore("), "Should have restore method");
    assert!(
        code.contains("list_with_deleted"),
        "Should have list_with_deleted method"
    );
}

#[test]
fn test_codegen_no_soft_delete_without_directive() {
    let input = r#"
Product {
id: +uuid
name: string
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Should NOT have soft delete features
    assert!(
        !code.contains("deleted_at:"),
        "Should not have deleted_at field without directive"
    );
    assert!(
        !code.contains("fn restore("),
        "Should not have restore method without directive"
    );
}

#[test]
fn test_codegen_generates_batch_operations() {
    let input = r#"
User {
id: +uuid
email: &string
name: string
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Check for batch operation methods
    assert!(
        code.contains("fn insert_batch("),
        "Should have insert_batch method"
    );
    assert!(
        code.contains("fn delete_batch("),
        "Should have delete_batch method"
    );
}

#[test]
fn test_openapi_includes_batch_endpoints() {
    let input = r#"
User {
id: +uuid
email: &string
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let files = sinkdb::OpenApiGenerator::generate(&schema);
    let openapi_file = files
        .iter()
        .find(|f| f.path.contains("openapi.json"))
        .unwrap();

    // Check for batch endpoints
    assert!(
        openapi_file.content.contains("/api/users/batch"),
        "Should have batch endpoint (note: plural model name)"
    );
    assert!(
        openapi_file.content.contains("Batch create"),
        "Should document batch create"
    );
    assert!(
        openapi_file.content.contains("Batch delete"),
        "Should document batch delete"
    );
}

#[test]
fn test_openapi_includes_field_selection_param() {
    let input = r#"
User {
id: +uuid
email: &string
name: string
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let files = sinkdb::OpenApiGenerator::generate(&schema);
    let openapi_file = files
        .iter()
        .find(|f| f.path.contains("openapi.json"))
        .unwrap();

    // Check for fields parameter
    assert!(
        openapi_file.content.contains("\"fields\""),
        "Should have fields parameter"
    );
    assert!(
        openapi_file
            .content
            .contains("Comma-separated list of fields"),
        "Should document fields parameter"
    );
}

#[test]
fn test_openapi_includes_include_deleted_for_soft_delete() {
    let input = r#"
User {
id: +uuid
email: &string
@soft_delete
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let files = sinkdb::OpenApiGenerator::generate(&schema);
    let openapi_file = files
        .iter()
        .find(|f| f.path.contains("openapi.json"))
        .unwrap();

    // Check for include_deleted parameter
    assert!(
        openapi_file.content.contains("\"include_deleted\""),
        "Should have include_deleted parameter for soft delete model"
    );
    assert!(
        openapi_file
            .content
            .contains("Include soft-deleted records"),
        "Should document include_deleted parameter"
    );
}

#[test]
fn test_multiple_materialized_fields() {
    let input = r#"
Order {
id: +uuid
subtotal: f64 @computed @materialized
tax: f64 @computed @materialized
total: f64 @computed @materialized
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    let materialized_count = model.fields.iter().filter(|f| f.is_materialized).count();

    assert_eq!(materialized_count, 3, "Should have 3 materialized fields");

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Verify all three fields get storage
    assert!(code.contains("materialized_subtotal"));
    assert!(code.contains("materialized_tax"));
    assert!(code.contains("materialized_total"));
}

#[test]
fn test_soft_delete_and_materialized_together() {
    let input = r#"
Product {
id: +uuid
name: string
price: f64
quantity: u32
total_value: f64 @computed @materialized
@soft_delete
}
"#;

    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();

    let model = &schema.models[0];
    assert!(model.soft_delete);

    let total_value = model
        .fields
        .iter()
        .find(|f| f.name == "total_value")
        .unwrap();
    assert!(total_value.is_materialized);

    let codegen = CodeGenerator::new();
    let code = codegen.generate(&schema);

    // Both features should be present
    assert!(code.contains("materialized_total_value"));
    assert!(code.contains("deleted_at"));
    assert!(code.contains("fn restore("));
}
