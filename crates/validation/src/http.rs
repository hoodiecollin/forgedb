//! HTTP-specific validation extensions for Sprint 9

use crate::ValidationError;

/// HTTP validation error that maps to status codes
#[derive(Debug, Clone)]
pub struct HttpValidationError {
    pub status_code: u16,
    pub errors: Vec<ValidationError>,
}

impl HttpValidationError {
    /// Create a bad request error (400)
    pub fn bad_request(errors: Vec<ValidationError>) -> Self {
        Self {
            status_code: 400,
            errors,
        }
    }

    /// Create a not found error (404)
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status_code: 404,
            errors: vec![ValidationError::new(message)],
        }
    }

    /// Create a conflict error (409)
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status_code: 409,
            errors: vec![ValidationError::new(message)],
        }
    }

    /// Create an unprocessable entity error (422)
    pub fn unprocessable_entity(errors: Vec<ValidationError>) -> Self {
        Self {
            status_code: 422,
            errors,
        }
    }

    /// Create an internal server error (500)
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            status_code: 500,
            errors: vec![ValidationError::new(message)],
        }
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        self.status_code >= 400 && self.status_code < 500
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        self.status_code >= 500 && self.status_code < 600
    }

    /// Get the primary error message
    pub fn message(&self) -> String {
        if self.errors.is_empty() {
            "Unknown error".to_string()
        } else {
            self.errors[0].message.clone()
        }
    }
}

impl std::fmt::Display for HttpValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {} - {}", self.status_code, self.message())
    }
}

impl std::error::Error for HttpValidationError {}

/// Validation rules for HTTP requests
pub struct HttpValidator;

impl HttpValidator {
    /// Validate required fields are present
    pub fn validate_required_fields(
        fields: &[(&str, Option<&str>)],
    ) -> Result<(), Vec<ValidationError>> {
        let errors: Vec<ValidationError> = fields
            .iter()
            .filter_map(|(name, value)| {
                if value.is_none() || value.unwrap().is_empty() {
                    Some(ValidationError::new(format!("Field '{}' is required", name)))
                } else {
                    None
                }
            })
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate email format (basic check)
    pub fn validate_email(email: &str) -> Result<(), ValidationError> {
        if email.contains('@') && email.contains('.') && email.len() >= 5 {
            Ok(())
        } else {
            Err(ValidationError::new("Invalid email format")
                .with_suggestion("Email must contain @ and domain"))
        }
    }

    /// Validate string length
    pub fn validate_length(
        field_name: &str,
        value: &str,
        min: usize,
        max: usize,
    ) -> Result<(), ValidationError> {
        let len = value.len();
        if len < min {
            Err(ValidationError::new(format!(
                "Field '{}' must be at least {} characters",
                field_name, min
            )))
        } else if len > max {
            Err(ValidationError::new(format!(
                "Field '{}' must be at most {} characters",
                field_name, max
            )))
        } else {
            Ok(())
        }
    }

    /// Validate numeric range
    pub fn validate_range<T: PartialOrd + std::fmt::Display>(
        field_name: &str,
        value: T,
        min: T,
        max: T,
    ) -> Result<(), ValidationError> {
        if value < min {
            Err(ValidationError::new(format!(
                "Field '{}' must be at least {}",
                field_name, min
            )))
        } else if value > max {
            Err(ValidationError::new(format!(
                "Field '{}' must be at most {}",
                field_name, max
            )))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
