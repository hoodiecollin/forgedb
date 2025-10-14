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

#[cfg(test)]
mod tests {
    use super::*;
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
            Ok(self.records.iter().map(|(_, model)| model.clone()).collect())
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
            if let Some((_, model)) = self.records.iter_mut().find(|(record_id, _)| record_id == id)
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
    fn test_list_operation() {
        let storage = TestStorage { records: vec![] };
        let result = ListOperation::execute(&storage).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_get_operation() {
        let storage = TestStorage { records: vec![] };
        let id = Uuid::new_v4();
        let result = GetOperation::execute(&storage, &id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_create_operation() {
        let mut storage = TestStorage { records: vec![] };
        let result = CreateOperation::execute(
            &mut storage,
            CreateTestModel {
                name: "Test".to_string(),
            },
        )
        .unwrap();
        assert_eq!(result.name, "Test");
    }

    #[test]
    fn test_update_operation() {
        let mut storage = TestStorage { records: vec![] };
        let created = CreateOperation::execute(
            &mut storage,
            CreateTestModel {
                name: "Original".to_string(),
            },
        )
        .unwrap();

        let updated = UpdateOperation::execute(
            &mut storage,
            &created.id,
            UpdateTestModel {
                name: Some("Updated".to_string()),
            },
        )
        .unwrap();
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().name, "Updated");
    }

    #[test]
    fn test_delete_operation() {
        let mut storage = TestStorage { records: vec![] };
        let created = CreateOperation::execute(
            &mut storage,
            CreateTestModel {
                name: "ToDelete".to_string(),
            },
        )
        .unwrap();

        let deleted = DeleteOperation::execute(&mut storage, &created.id).unwrap();
        assert!(deleted);

        let result = GetOperation::execute(&storage, &created.id).unwrap();
        assert!(result.is_none());
    }
}
