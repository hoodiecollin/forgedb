//! HTTP handlers for CRUD operations

use crate::CrudOperations;
use serde::Serialize;
use std::fmt;
use uuid::Uuid;

/// Result type for CRUD operations
pub type CrudResult<T> = Result<T, CrudError>;

/// Errors that can occur during CRUD operations
#[derive(Debug)]
pub enum CrudError {
    /// Resource not found
    NotFound(String),
    /// Validation error
    ValidationError(String),
    /// Conflict (e.g., unique constraint violation)
    Conflict(String),
    /// Internal error
    Internal(String),
}

impl fmt::Display for CrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CrudError::NotFound(msg) => write!(f, "Not found: {}", msg),
            CrudError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            CrudError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            CrudError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for CrudError {}

/// Generic CRUD handlers that can be used with any storage implementing CrudOperations
pub struct CrudHandlers<T: CrudOperations> {
    storage: T,
}

impl<T: CrudOperations> CrudHandlers<T> {
    /// Create new handlers with the given storage
    pub fn new(storage: T) -> Self {
        Self { storage }
    }

    /// List all records
    pub fn list(&self) -> CrudResult<Vec<T::Model>> {
        self.storage.list()
    }

    /// Get a record by ID
    pub fn get(&self, id: &Uuid) -> CrudResult<T::Model> {
        self.storage
            .get(id)?
            .ok_or_else(|| CrudError::NotFound(format!("Record with id {} not found", id)))
    }

    /// Create a new record
    pub fn create(&mut self, input: T::CreateInput) -> CrudResult<T::Model> {
        self.storage.create(input)
    }

    /// Update an existing record
    pub fn update(&mut self, id: &Uuid, input: T::UpdateInput) -> CrudResult<T::Model> {
        self.storage
            .update(id, input)?
            .ok_or_else(|| CrudError::NotFound(format!("Record with id {} not found", id)))
    }

    /// Delete a record
    pub fn delete(&mut self, id: &Uuid) -> CrudResult<()> {
        let deleted = self.storage.delete(id)?;
        if deleted {
            Ok(())
        } else {
            Err(CrudError::NotFound(format!(
                "Record with id {} not found",
                id
            )))
        }
    }

    /// Get the count of records
    pub fn count(&self) -> CrudResult<usize> {
        self.storage.count()
    }

    /// Get a reference to the underlying storage
    pub fn storage(&self) -> &T {
        &self.storage
    }

    /// Get a mutable reference to the underlying storage
    pub fn storage_mut(&mut self) -> &mut T {
        &mut self.storage
    }
}

/// Response wrapper for list operations with metadata
/// Standardized format: {data, total, limit, offset}
#[derive(Debug, Serialize)]
pub struct ListResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

impl<T: Serialize> ListResponse<T> {
    /// Create a new list response with just data (no pagination info)
    pub fn new(data: Vec<T>) -> Self {
        let total = data.len();
        Self {
            data,
            total,
            limit: None,
            offset: None,
        }
    }

    /// Create a new list response with pagination info
    pub fn with_pagination(data: Vec<T>, total: usize, limit: usize, offset: usize) -> Self {
        Self {
            data,
            total,
            limit: Some(limit),
            offset: Some(offset),
        }
    }

    /// Create from data with explicit total count (for when total differs from data.len())
    pub fn with_total(data: Vec<T>, total: usize) -> Self {
        Self {
            data,
            total,
            limit: None,
            offset: None,
        }
    }
}

