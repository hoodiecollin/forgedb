//! ForgeDB CRUD API
//!
//! Generic CRUD operation handlers for database models.
//! Provides traits and implementations for list, get, create, update, and delete operations.

mod handlers;
mod operations;

pub use handlers::{CrudHandlers, CrudResult};
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_crud_operations() {
        let mut storage = TestStorage { records: vec![] };

        // Create
        let model = storage
            .create(CreateTestModel {
                name: "Test".to_string(),
            })
            .unwrap();
        assert_eq!(model.name, "Test");

        // Get
        let retrieved = storage.get(&model.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");

        // List
        let all = storage.list().unwrap();
        assert_eq!(all.len(), 1);

        // Update
        let updated = storage
            .update(
                &model.id,
                UpdateTestModel {
                    name: Some("Updated".to_string()),
                },
            )
            .unwrap();
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().name, "Updated");

        // Delete
        let deleted = storage.delete(&model.id).unwrap();
        assert!(deleted);

        // Verify deletion
        let after_delete = storage.get(&model.id).unwrap();
        assert!(after_delete.is_none());
    }

    #[test]
    fn test_count() {
        let mut storage = TestStorage { records: vec![] };

        assert_eq!(storage.count().unwrap(), 0);

        storage
            .create(CreateTestModel {
                name: "Test1".to_string(),
            })
            .unwrap();
        assert_eq!(storage.count().unwrap(), 1);

        storage
            .create(CreateTestModel {
                name: "Test2".to_string(),
            })
            .unwrap();
        assert_eq!(storage.count().unwrap(), 2);
    }
}
