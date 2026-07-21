//! Error types and HTTP error responses

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// API error types
#[derive(Debug)]
pub enum ApiError {
    /// Validation error (400)
    ValidationError {
        message: String,
        details: Vec<ErrorDetail>,
    },
    /// Resource not found (404)
    NotFound { resource: String, id: String },
    /// Conflict - unique constraint violation (409)
    Conflict { message: String, field: String },
    /// Internal server error (500)
    InternalError { message: String },
}

/// Error detail for validation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub field: String,
    pub message: String,
}

/// Standard error response format
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ErrorDetail>,
}

impl ApiError {
    /// Create a validation error
    pub fn validation(message: impl Into<String>, details: Vec<ErrorDetail>) -> Self {
        Self::ValidationError {
            message: message.into(),
            details,
        }
    }

    /// Create a not found error
    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
            id: id.into(),
        }
    }

    /// Create a conflict error
    pub fn conflict(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
            field: field.into(),
        }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::InternalError {
            message: message.into(),
        }
    }

    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::ValidationError { .. } => StatusCode::BAD_REQUEST,
            ApiError::NotFound { .. } => StatusCode::NOT_FOUND,
            ApiError::Conflict { .. } => StatusCode::CONFLICT,
            ApiError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Convert to ErrorResponse
    pub fn to_response(&self) -> ErrorResponse {
        match self {
            ApiError::ValidationError { message, details } => ErrorResponse {
                error: ErrorInfo {
                    code: "VALIDATION_ERROR".to_string(),
                    message: message.clone(),
                    details: details.clone(),
                },
            },
            ApiError::NotFound { resource, id } => ErrorResponse {
                error: ErrorInfo {
                    code: "NOT_FOUND".to_string(),
                    message: format!("{} with id '{}' not found", resource, id),
                    details: vec![],
                },
            },
            ApiError::Conflict { message, field } => ErrorResponse {
                error: ErrorInfo {
                    code: "CONFLICT".to_string(),
                    message: message.clone(),
                    details: vec![ErrorDetail {
                        field: field.clone(),
                        message: "Already exists".to_string(),
                    }],
                },
            },
            ApiError::InternalError { message } => ErrorResponse {
                error: ErrorInfo {
                    code: "INTERNAL_ERROR".to_string(),
                    message: message.clone(),
                    details: vec![],
                },
            },
        }
    }
}

// Implement IntoResponse for ApiError to work with Axum handlers
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(self.to_response());
        (status, body).into_response()
    }
}
