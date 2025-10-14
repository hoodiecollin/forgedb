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
#[derive(Debug, Serialize)]
pub struct ListResponse<T: Serialize> {
    pub data: Vec<T>,
    pub count: usize,
}

impl<T: Serialize> ListResponse<T> {
    pub fn new(data: Vec<T>) -> Self {
        let count = data.len();
        Self { data, count }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrudOperations;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestModel {
        id: Uuid,
        name: String,
    }

    #[derive(Debug, Deserialize)]
    struct CreateTestModel {
        name: String,
    }

    #[derive(Debug, Deserialize)]
    struct UpdateTestModel {
        name: Option<String>,
    }

    struct TestStorage {
        records: Vec<(Uuid, TestModel)>,
    }

    impl CrudOperations for TestStorage {
        type Model = TestModel;
        type CreateInput = CreateTestModel;
        type UpdateInput = UpdateTestModel;

        fn list(&self) -> CrudResult<Vec<Self::Model>> {
            Ok(self
                .records
                .iter()
                .map(|(_, model)| model.clone())
                .collect())
        }

        fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
            Ok(self
                .records
                .iter()
                .find(|(record_id, _)| record_id == id)
                .map(|(_, model)| model.clone()))
        }

        fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
            let id = Uuid::new_v4();
            let model = TestModel {
                id,
                name: input.name,
            };
            self.records.push((id, model.clone()));
            Ok(model)
        }

        fn update(
            &mut self,
            id: &Uuid,
            input: Self::UpdateInput,
        ) -> CrudResult<Option<Self::Model>> {
            if let Some((_, model)) = self
                .records
                .iter_mut()
                .find(|(record_id, _)| record_id == id)
            {
                if let Some(name) = input.name {
                    model.name = name;
                }
                Ok(Some(model.clone()))
            } else {
                Ok(None)
            }
        }

        fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
            let initial_len = self.records.len();
            self.records.retain(|(record_id, _)| record_id != id);
            Ok(self.records.len() < initial_len)
        }
    }

    #[test]
    fn test_handlers_list() {
        let storage = TestStorage { records: vec![] };
        let handlers = CrudHandlers::new(storage);

        let result = handlers.list().unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_handlers_create_and_get() {
        let storage = TestStorage { records: vec![] };
        let mut handlers = CrudHandlers::new(storage);

        let created = handlers
            .create(CreateTestModel {
                name: "Test".to_string(),
            })
            .unwrap();
        assert_eq!(created.name, "Test");

        let retrieved = handlers.get(&created.id).unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test");
    }

    #[test]
    fn test_handlers_update() {
        let storage = TestStorage { records: vec![] };
        let mut handlers = CrudHandlers::new(storage);

        let created = handlers
            .create(CreateTestModel {
                name: "Original".to_string(),
            })
            .unwrap();

        let updated = handlers
            .update(
                &created.id,
                UpdateTestModel {
                    name: Some("Updated".to_string()),
                },
            )
            .unwrap();
        assert_eq!(updated.name, "Updated");
    }

    #[test]
    fn test_handlers_delete() {
        let storage = TestStorage { records: vec![] };
        let mut handlers = CrudHandlers::new(storage);

        let created = handlers
            .create(CreateTestModel {
                name: "ToDelete".to_string(),
            })
            .unwrap();

        handlers.delete(&created.id).unwrap();

        let result = handlers.get(&created.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_handlers_not_found() {
        let storage = TestStorage { records: vec![] };
        let handlers = CrudHandlers::new(storage);

        let id = Uuid::new_v4();
        let result = handlers.get(&id);
        assert!(result.is_err());
        assert!(matches!(result, Err(CrudError::NotFound(_))));
    }

    #[test]
    fn test_list_response() {
        let data = vec![TestModel {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
        }];
        let response = ListResponse::new(data);
        assert_eq!(response.count, 1);
    }
}
