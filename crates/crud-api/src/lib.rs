//! ForgeDB CRUD API
//!
//! Generic CRUD operation handlers for database models.
//! Provides traits and implementations for list, get, create, update, and delete operations.

mod handlers;
mod operations;

pub use handlers::{CrudError, CrudHandlers, CrudResult, ListResponse};
pub use operations::{
    CreateOperation, DeleteOperation, GetOperation, ListOperation, UpdateOperation,
};

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use uuid::Uuid;

/// Generic CRUD operations trait that storage types can implement
pub trait CrudOperations {
    /// The type of the model (e.g., User, Post)
    type Model: Clone + Debug + Serialize + for<'de> Deserialize<'de>;

    /// The type used for creating (may have fewer fields than Model)
    type CreateInput: Debug + for<'de> Deserialize<'de>;

    /// The type used for updating (fields are optional)
    type UpdateInput: Debug + for<'de> Deserialize<'de>;

    /// List all records (excluding tombstoned records)
    fn list(&self) -> CrudResult<Vec<Self::Model>>;

    /// Get a record by ID
    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>>;

    /// Create a new record
    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model>;

    /// Update an existing record
    fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>>;

    /// Delete a record (marks as tombstoned)
    fn delete(&mut self, id: &Uuid) -> CrudResult<bool>;

    /// Get the count of non-tombstoned records
    fn count(&self) -> CrudResult<usize> {
        Ok(self.list()?.len())
    }
}

