use forgedb_http_server::*;

#[test]
fn test_record_http_request() {
    record_http_request("GET", "/api/users", 200, 0.05);
    // Metrics should be recorded without panicking
}

#[test]
fn test_record_db_operation() {
    record_db_operation("select", "User", 0.001);
    // Metrics should be recorded without panicking
}

#[test]
fn test_record_error() {
    record_error("validation", "BAD_REQUEST");
    // Metrics should be recorded without panicking
}
