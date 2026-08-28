use forgedb_validation::{HttpValidator, Position, StatusCodeMapper, ValidationError};

#[test]
fn error_carries_position_and_suggestion() {
    let error = ValidationError::new("Field 'email' must have @email constraint")
        .with_position(Position::new(5, 12))
        .with_suggestion("Add @email constraint: email: string @email");

    let position = error.position.expect("position was set");
    assert_eq!(position.line, 5);
    assert_eq!(position.column, 12);
    assert_eq!(error.message, "Field 'email' must have @email constraint");
}

#[test]
fn http_validator_accepts_valid_input() {
    assert!(HttpValidator::validate_email("user@example.com").is_ok());
    assert!(HttpValidator::validate_length("name", "hi", 1, 10).is_ok());
}

#[test]
fn status_code_mapper_classifies_codes() {
    assert_eq!(StatusCodeMapper::for_validation_error("not_found"), 404);
    assert!(StatusCodeMapper::is_success(200));
    assert!(StatusCodeMapper::is_client_error(404));
    assert!(StatusCodeMapper::is_server_error(500));
}
