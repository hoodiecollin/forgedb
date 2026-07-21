use forgedb_validation::*;
use forgedb_validation::http::*;

#[test]
fn test_http_validation_error_bad_request() {
    let error = HttpValidationError::bad_request(vec![ValidationError::new("Invalid input")]);
    assert_eq!(error.status_code, 400);
    assert!(error.is_client_error());
    assert!(!error.is_server_error());
}

#[test]
fn test_http_validation_error_not_found() {
    let error = HttpValidationError::not_found("Resource not found");
    assert_eq!(error.status_code, 404);
    assert!(error.is_client_error());
}

#[test]
fn test_http_validation_error_conflict() {
    let error = HttpValidationError::conflict("Email already exists");
    assert_eq!(error.status_code, 409);
    assert!(error.is_client_error());
}

#[test]
fn test_http_validation_error_internal() {
    let error = HttpValidationError::internal_error("Database connection failed");
    assert_eq!(error.status_code, 500);
    assert!(!error.is_client_error());
    assert!(error.is_server_error());
}

#[test]
fn test_validate_required_fields() {
    let fields = vec![
        ("name", Some("John")),
        ("email", Some("")), // Empty counts as missing
        ("age", None),
    ];
    let result = HttpValidator::validate_required_fields(&fields);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 2); // email and age
}

#[test]
fn test_validate_required_fields_success() {
    let fields = vec![("name", Some("John")), ("email", Some("john@example.com"))];
    let result = HttpValidator::validate_required_fields(&fields);
    assert!(result.is_ok());
}

#[test]
fn test_validate_email() {
    assert!(HttpValidator::validate_email("test@example.com").is_ok());
    assert!(HttpValidator::validate_email("user@domain.co.uk").is_ok());

    assert!(HttpValidator::validate_email("invalid").is_err());
    assert!(HttpValidator::validate_email("no@domain").is_err());
    assert!(HttpValidator::validate_email("nodomain.com").is_err());
}

#[test]
fn test_validate_length() {
    assert!(HttpValidator::validate_length("name", "John", 2, 10).is_ok());
    assert!(HttpValidator::validate_length("name", "J", 2, 10).is_err());
    assert!(HttpValidator::validate_length("name", "VeryLongName", 2, 10).is_err());
}

#[test]
fn test_validate_range() {
    assert!(HttpValidator::validate_range("age", 25, 0, 150).is_ok());
    assert!(HttpValidator::validate_range("age", -1, 0, 150).is_err());
    assert!(HttpValidator::validate_range("age", 200, 0, 150).is_err());
}
