use forgedb_crud_api::*;
use uuid::Uuid;
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
    assert_eq!(response.total, 1);
    assert_eq!(response.limit, 100);
    assert_eq!(response.offset, 0);
}
