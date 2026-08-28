use forgedb_validation::*;

#[test]
fn test_is_snake_case() {
    assert!(is_snake_case("user_name"));
    assert!(is_snake_case("user_name_123"));
    assert!(is_snake_case("user123"));
    assert!(is_snake_case("_private"));
    assert!(is_snake_case("email"));

    assert!(!is_snake_case("UserName"));
    assert!(!is_snake_case("userName"));
    assert!(!is_snake_case("user-name"));
    assert!(!is_snake_case(""));
    assert!(!is_snake_case("User_name"));
}

#[test]
fn test_is_pascal_case() {
    assert!(is_pascal_case("UserName"));
    assert!(is_pascal_case("User"));
    assert!(is_pascal_case("User123"));
    assert!(is_pascal_case("MyModel"));

    assert!(!is_pascal_case("userName"));
    assert!(!is_pascal_case("user_name"));
    assert!(!is_pascal_case("User_Name"));
    assert!(!is_pascal_case(""));
    assert!(!is_pascal_case("user"));
}

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("UserName"), "user_name");
    assert_eq!(to_snake_case("User"), "user");
    assert_eq!(to_snake_case("MyModel"), "my_model");
    assert_eq!(to_snake_case("HTTPServer"), "http_server");
    assert_eq!(to_snake_case("user_name"), "user_name");
    assert_eq!(to_snake_case("userId"), "user_id");
    assert_eq!(to_snake_case("ID"), "id");
}

#[test]
fn test_to_pascal_case() {
    assert_eq!(to_pascal_case("user_name"), "UserName");
    assert_eq!(to_pascal_case("user"), "User");
    assert_eq!(to_pascal_case("my_model"), "MyModel");
    assert_eq!(to_pascal_case("UserName"), "UserName");
}

#[test]
fn test_validate_field_name_valid() {
    assert!(validate_field_name("user_name", None).is_ok());
    assert!(validate_field_name("email", None).is_ok());
    assert!(validate_field_name("age_123", None).is_ok());
}

#[test]
fn test_validate_field_name_invalid() {
    let result = validate_field_name("UserName", None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("snake_case"));
    assert!(err.suggestion.is_some());
    assert!(err.suggestion.unwrap().contains("user_name"));
}

#[test]
fn test_validate_field_name_with_position() {
    let pos = Position::new(10, 5);
    let result = validate_field_name("BadName", Some(pos));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.position, Some(pos));
}

#[test]
fn test_validate_model_name_valid() {
    assert!(validate_model_name("User", None).is_ok());
    assert!(validate_model_name("UserModel", None).is_ok());
    assert!(validate_model_name("MyModel123", None).is_ok());
}

#[test]
fn test_validate_model_name_invalid() {
    let result = validate_model_name("user_model", None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("PascalCase"));
    assert!(err.suggestion.is_some());
    assert!(err.suggestion.unwrap().contains("UserModel"));
}

#[test]
fn test_validate_model_name_with_position() {
    let pos = Position::new(5, 1);
    let result = validate_model_name("bad_name", Some(pos));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.position, Some(pos));
}

#[test]
fn test_check_duplicate_fields_no_duplicates() {
    let fields = vec![
        ("email".to_string(), None),
        ("username".to_string(), None),
        ("age".to_string(), None),
    ];
    assert!(check_duplicate_fields(&fields).is_ok());
}

#[test]
fn test_check_duplicate_fields_with_duplicates() {
    let fields = vec![
        ("email".to_string(), None),
        ("username".to_string(), None),
        ("email".to_string(), Some(Position::new(5, 3))),
    ];
    let result = check_duplicate_fields(&fields);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("Duplicate field name 'email'"));
    assert_eq!(err.position, Some(Position::new(5, 3)));
}

#[test]
fn test_check_duplicate_models_no_duplicates() {
    let models = vec![
        ("User".to_string(), None),
        ("Post".to_string(), None),
        ("Comment".to_string(), None),
    ];
    assert!(check_duplicate_models(&models).is_ok());
}

#[test]
fn test_check_duplicate_models_with_duplicates() {
    let models = vec![
        ("User".to_string(), None),
        ("Post".to_string(), None),
        ("User".to_string(), Some(Position::new(20, 1))),
    ];
    let result = check_duplicate_models(&models);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("Duplicate model name 'User'"));
    assert_eq!(err.position, Some(Position::new(20, 1)));
}

#[test]
fn test_validation_error_display_with_position() {
    let error = ValidationError::new("Test error")
        .with_position(Position::new(10, 5))
        .with_suggestion("Try this instead");

    let display = format!("{}", error);
    assert!(display.contains("line 10"));
    assert!(display.contains("column 5"));
    assert!(display.contains("Test error"));
    assert!(display.contains("Try this instead"));
}

#[test]
fn test_validation_error_display_without_position() {
    let error = ValidationError::new("Test error").with_suggestion("Try this instead");

    let display = format!("{}", error);
    assert!(!display.contains("line"));
    assert!(display.contains("Test error"));
    assert!(display.contains("Try this instead"));
}

#[test]
fn test_snake_case_edge_cases() {
    assert!(is_snake_case("a"));
    assert!(is_snake_case("x"));
    assert!(!is_snake_case("A"));

    assert!(is_snake_case("_private"));
    assert!(is_snake_case("__double"));

    assert!(is_snake_case("field123"));
    assert!(is_snake_case("abc_123_def"));
    assert!(!is_snake_case("123field"));

    assert!(is_snake_case("a__b"));
    assert!(is_snake_case("___"));

    assert!(!is_snake_case("camelCase"));
    assert!(!is_snake_case("SCREAMING_CASE"));
    assert!(!is_snake_case("kebab-case"));
    assert!(!is_snake_case("dot.case"));
}

#[test]
fn test_pascal_case_edge_cases() {
    assert!(is_pascal_case("A"));
    assert!(is_pascal_case("X"));
    assert!(!is_pascal_case("a"));

    assert!(is_pascal_case("Model123"));
    assert!(is_pascal_case("HTTP2Server"));
    assert!(!is_pascal_case("123Model"));

    assert!(!is_pascal_case("snake_case"));
    assert!(is_pascal_case("SCREAMING"));
    assert!(!is_pascal_case("kebab-case"));
    assert!(!is_pascal_case("dot.case"));
    assert!(!is_pascal_case("camelCase"));
}

#[test]
fn test_to_snake_case_edge_cases() {
    assert_eq!(to_snake_case("already_snake"), "already_snake");

    assert_eq!(to_snake_case("A"), "a");

    assert_eq!(to_snake_case("XMLParser"), "xml_parser");
    assert_eq!(to_snake_case("HTMLElement"), "html_element");
    assert_eq!(to_snake_case("HTTPAPI"), "httpapi");

    assert_eq!(to_snake_case("User123"), "user123");
    assert_eq!(to_snake_case("HTML5Parser"), "html5_parser");

    assert_eq!(to_snake_case("camelCase"), "camel_case");
    assert_eq!(to_snake_case("camelCaseExample"), "camel_case_example");

    assert_eq!(to_snake_case("User_Name"), "user_name");
}

#[test]
fn test_to_pascal_case_edge_cases() {
    assert_eq!(to_pascal_case("AlreadyPascal"), "AlreadyPascal");

    assert_eq!(to_pascal_case("a"), "A");

    assert_eq!(to_pascal_case("a__b"), "AB");
    assert_eq!(to_pascal_case("___test"), "Test");

    assert_eq!(to_pascal_case("_private"), "Private");

    assert_eq!(to_pascal_case("field_123"), "Field123");

    assert_eq!(to_pascal_case("_"), "");
}

#[test]
fn test_duplicate_fields_edge_cases() {
    let empty: Vec<(String, Option<Position>)> = vec![];
    assert!(check_duplicate_fields(&empty).is_ok());

    let single = vec![("field".to_string(), None)];
    assert!(check_duplicate_fields(&single).is_ok());

    let case_sensitive = vec![("email".to_string(), None), ("Email".to_string(), None)];
    assert!(check_duplicate_fields(&case_sensitive).is_ok());

    let three_duplicates = vec![
        ("name".to_string(), None),
        ("name".to_string(), Some(Position::new(5, 1))),
        ("name".to_string(), Some(Position::new(10, 1))),
    ];
    let result = check_duplicate_fields(&three_duplicates);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.position, Some(Position::new(5, 1)));
}

#[test]
fn test_duplicate_models_edge_cases() {
    let empty: Vec<(String, Option<Position>)> = vec![];
    assert!(check_duplicate_models(&empty).is_ok());

    let single = vec![("User".to_string(), None)];
    assert!(check_duplicate_models(&single).is_ok());

    let case_sensitive = vec![("User".to_string(), None), ("user".to_string(), None)];
    assert!(check_duplicate_models(&case_sensitive).is_ok());
}

#[test]
fn test_validation_error_builder() {
    let err1 = ValidationError::new("Simple error");
    assert_eq!(err1.message, "Simple error");
    assert_eq!(err1.position, None);
    assert_eq!(err1.suggestion, None);

    let err2 = ValidationError::new("Error with position").with_position(Position::new(5, 10));
    assert_eq!(err2.position, Some(Position::new(5, 10)));
    assert_eq!(err2.suggestion, None);

    let err3 = ValidationError::new("Error with suggestion").with_suggestion("Fix it this way");
    assert_eq!(err3.position, None);
    assert!(err3.suggestion.is_some());

    let err4 = ValidationError::new("Full error")
        .with_position(Position::new(1, 1))
        .with_suggestion("Try this");
    assert!(err4.position.is_some());
    assert!(err4.suggestion.is_some());
}

#[test]
fn test_validate_edge_case_names() {
    assert!(validate_field_name("x", None).is_ok());
    assert!(validate_field_name("a", None).is_ok());
    assert!(validate_field_name("_", None).is_ok());

    assert!(validate_model_name("X", None).is_ok());
    assert!(validate_model_name("A", None).is_ok());

    let long_field = "very_long_field_name_with_many_underscores_that_is_still_valid";
    assert!(validate_field_name(long_field, None).is_ok());

    let long_model = "VeryLongModelNameWithManyCamelCaseWordsThatIsStillValid";
    assert!(validate_model_name(long_model, None).is_ok());
}
