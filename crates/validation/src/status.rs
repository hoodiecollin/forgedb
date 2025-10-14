//! HTTP status code mapping for validation errors

/// Map common validation errors to HTTP status codes
pub struct StatusCodeMapper;

impl StatusCodeMapper {
    /// Get status code for validation error type
    pub fn for_validation_error(error_type: &str) -> u16 {
        match error_type {
            "required_field" => 400,       // Bad Request
            "invalid_format" => 400,       // Bad Request
            "invalid_type" => 400,         // Bad Request
            "out_of_range" => 400,         // Bad Request
            "not_found" => 404,            // Not Found
            "already_exists" => 409,       // Conflict
            "unique_violation" => 409,     // Conflict
            "foreign_key_violation" => 422, // Unprocessable Entity
            "internal_error" => 500,       // Internal Server Error
            _ => 400,                      // Default to Bad Request
        }
    }

    /// Get status code name
    pub fn status_name(code: u16) -> &'static str {
        match code {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            422 => "Unprocessable Entity",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Unknown",
        }
    }

    /// Check if status code indicates success
    pub fn is_success(code: u16) -> bool {
        code >= 200 && code < 300
    }

    /// Check if status code indicates client error
    pub fn is_client_error(code: u16) -> bool {
        code >= 400 && code < 500
    }

    /// Check if status code indicates server error
    pub fn is_server_error(code: u16) -> bool {
        code >= 500 && code < 600
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_for_validation_error() {
        assert_eq!(StatusCodeMapper::for_validation_error("required_field"), 400);
        assert_eq!(StatusCodeMapper::for_validation_error("not_found"), 404);
        assert_eq!(StatusCodeMapper::for_validation_error("already_exists"), 409);
        assert_eq!(StatusCodeMapper::for_validation_error("internal_error"), 500);
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
}
