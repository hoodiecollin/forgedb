use forgedb_validation::StatusCodeMapper;

#[test]
fn test_for_validation_error() {
    assert_eq!(
        StatusCodeMapper::for_validation_error("required_field"),
        400
    );
    assert_eq!(StatusCodeMapper::for_validation_error("not_found"), 404);
    assert_eq!(
        StatusCodeMapper::for_validation_error("already_exists"),
        409
    );
    assert_eq!(
        StatusCodeMapper::for_validation_error("internal_error"),
        500
    );
    assert_eq!(StatusCodeMapper::for_validation_error("unknown"), 400);
}

#[test]
fn test_status_name() {
    assert_eq!(StatusCodeMapper::status_name(200), "OK");
    assert_eq!(StatusCodeMapper::status_name(404), "Not Found");
    assert_eq!(StatusCodeMapper::status_name(500), "Internal Server Error");
}

#[test]
fn test_is_success() {
    assert!(StatusCodeMapper::is_success(200));
    assert!(StatusCodeMapper::is_success(201));
    assert!(!StatusCodeMapper::is_success(400));
    assert!(!StatusCodeMapper::is_success(500));
}

#[test]
fn test_is_client_error() {
    assert!(!StatusCodeMapper::is_client_error(200));
    assert!(StatusCodeMapper::is_client_error(400));
    assert!(StatusCodeMapper::is_client_error(404));
    assert!(!StatusCodeMapper::is_client_error(500));
}

#[test]
fn test_is_server_error() {
    assert!(!StatusCodeMapper::is_server_error(200));
    assert!(!StatusCodeMapper::is_server_error(400));
    assert!(StatusCodeMapper::is_server_error(500));
    assert!(StatusCodeMapper::is_server_error(503));
}
