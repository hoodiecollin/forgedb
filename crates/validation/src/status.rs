//! HTTP status code mapping for validation errors

/// Map common validation errors to HTTP status codes
pub struct StatusCodeMapper;

impl StatusCodeMapper {
    /// Get status code for validation error type
    pub fn for_validation_error(error_type: &str) -> u16 {
        match error_type {
            "required_field" => 400,        // Bad Request
            "invalid_format" => 400,        // Bad Request
            "invalid_type" => 400,          // Bad Request
            "out_of_range" => 400,          // Bad Request
            "not_found" => 404,             // Not Found
            "already_exists" => 409,        // Conflict
            "unique_violation" => 409,      // Conflict
            "foreign_key_violation" => 422, // Unprocessable Entity
            "internal_error" => 500,        // Internal Server Error
            _ => 400,                       // Default to Bad Request
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

