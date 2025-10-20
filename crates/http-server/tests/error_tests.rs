use forgedb_http_server::*;

#[test]
fn test_validation_error() {
    let error = ApiError::validation(
        "Invalid input",
        vec![ErrorDetail {
            field: "email".to_string(),
            message: "Must be a valid email".to_string(),
        }],
    );
    assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);

    let response = error.to_response();
    assert_eq!(response.error.code, "VALIDATION_ERROR");
    assert_eq!(response.error.details.len(), 1);
}

#[test]
fn test_not_found_error() {
    let error = ApiError::not_found("User", "123");
    assert_eq!(error.status_code(), StatusCode::NOT_FOUND);

    let response = error.to_response();
    assert_eq!(response.error.code, "NOT_FOUND");
    assert!(response.error.message.contains("User"));
}

#[test]
fn test_conflict_error() {
    let error = ApiError::conflict("Email already exists", "email");
    assert_eq!(error.status_code(), StatusCode::CONFLICT);

    let response = error.to_response();
    assert_eq!(response.error.code, "CONFLICT");
}

#[test]
fn test_internal_error() {
    let error = ApiError::internal("Database connection failed");
    assert_eq!(error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    let response = error.to_response();
    assert_eq!(response.error.code, "INTERNAL_ERROR");
}
