//! Individual CRUD operation implementations

use crate::{CrudOperations, CrudResult};
use uuid::Uuid;

/// List operation - Get all records
pub struct ListOperation;

impl ListOperation {
    pub fn execute<T: CrudOperations>(storage: &T) -> CrudResult<Vec<T::Model>> {
        storage.list()
    }
}

/// Get operation - Get a single record by ID
pub struct GetOperation;

impl GetOperation {
    pub fn execute<T: CrudOperations>(storage: &T, id: &Uuid) -> CrudResult<Option<T::Model>> {
        storage.get(id)
    }
}

/// Create operation - Create a new record
pub struct CreateOperation;

impl CreateOperation {
    pub fn execute<T: CrudOperations>(
        storage: &mut T,
        input: T::CreateInput,
    ) -> CrudResult<T::Model> {
        storage.create(input)
    }
}

/// Update operation - Update an existing record
pub struct UpdateOperation;

impl UpdateOperation {
    pub fn execute<T: CrudOperations>(
        storage: &mut T,
        id: &Uuid,
        input: T::UpdateInput,
    ) -> CrudResult<Option<T::Model>> {
        storage.update(id, input)
    }
}

/// Delete operation - Delete a record
pub struct DeleteOperation;

impl DeleteOperation {
    pub fn execute<T: CrudOperations>(storage: &mut T, id: &Uuid) -> CrudResult<bool> {
        storage.delete(id)
    }
}

