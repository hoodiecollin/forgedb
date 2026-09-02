pub struct StatusCodeMapper;

impl StatusCodeMapper {
    pub fn for_validation_error(error_type: &str) -> u16 {
        match error_type {
            "required_field" => 400,
            "invalid_format" => 400,
            "invalid_type" => 400,
            "out_of_range" => 400,
            "not_found" => 404,
            "already_exists" => 409,
            "unique_violation" => 409,
            "foreign_key_violation" => 422,
            "internal_error" => 500,
            _ => 400,
        }
    }

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

    pub fn is_success(code: u16) -> bool {
        code >= 200 && code < 300
    }

    pub fn is_client_error(code: u16) -> bool {
        code >= 400 && code < 500
    }

    pub fn is_server_error(code: u16) -> bool {
        code >= 500 && code < 600
    }
}
