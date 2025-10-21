use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
fn test_error_not_found_display() {
    let error = CrudError::NotFound("Test resource".to_string());
    let error_string = format!("{}", error);
    assert!(error_string.contains("Not found"));
    assert!(error_string.contains("Test resource"));
}

#[test]
fn test_error_validation_display() {
    let error = CrudError::ValidationError("Invalid input".to_string());
    let error_string = format!("{}", error);
    assert!(error_string.contains("Validation error"));
    assert!(error_string.contains("Invalid input"));
}

#[test]
fn test_error_conflict_display() {
    let error = CrudError::Conflict("Duplicate key".to_string());
    let error_string = format!("{}", error);
    assert!(error_string.contains("Conflict"));
    assert!(error_string.contains("Duplicate key"));
}

#[test]
fn test_error_internal_display() {
    let error = CrudError::Internal("Database error".to_string());
    let error_string = format!("{}", error);
    assert!(error_string.contains("Internal error"));
    assert!(error_string.contains("Database error"));
}

#[test]
fn test_error_is_error_trait() {
    use std::error::Error;
    
    let error: Box<dyn Error> = Box::new(CrudError::NotFound("test".to_string()));
    assert!(error.to_string().contains("Not found"));
}

#[test]
fn test_handlers_get_not_found() {
    let storage = TestStorage { records: vec![] };
    let handlers = CrudHandlers::new(storage);

    let id = Uuid::new_v4();
    let result = handlers.get(&id);

    assert!(result.is_err());
    match result {
        Err(CrudError::NotFound(msg)) => {
            assert!(msg.contains(&id.to_string()));
        }
        _ => panic!("Expected NotFound error"),
    }
}

#[test]
fn test_handlers_update_not_found() {
    let storage = TestStorage { records: vec![] };
    let mut handlers = CrudHandlers::new(storage);

    let id = Uuid::new_v4();
    let result = handlers.update(
        &id,
        UpdateTestModel {
            name: Some("Test".to_string()),
        },
    );

    assert!(result.is_err());
    match result {
        Err(CrudError::NotFound(msg)) => {
            assert!(msg.contains(&id.to_string()));
        }
        _ => panic!("Expected NotFound error"),
    }
}

#[test]
fn test_handlers_delete_not_found() {
    let storage = TestStorage { records: vec![] };
    let mut handlers = CrudHandlers::new(storage);

    let id = Uuid::new_v4();
    let result = handlers.delete(&id);

    assert!(result.is_err());
    match result {
        Err(CrudError::NotFound(msg)) => {
            assert!(msg.contains(&id.to_string()));
        }
        _ => panic!("Expected NotFound error"),
    }
}

#[test]
fn test_handlers_storage_access() {
    let storage = TestStorage { records: vec![] };
    let handlers = CrudHandlers::new(storage);

    // Test immutable access
    let storage_ref = handlers.storage();
    assert_eq!(storage_ref.records.len(), 0);
}

#[test]
fn test_handlers_storage_mut_access() {
    let storage = TestStorage { records: vec![] };
    let mut handlers = CrudHandlers::new(storage);

    // Test mutable access
    let storage_mut = handlers.storage_mut();
    storage_mut.records.push((
        Uuid::new_v4(),
        TestModel {
            id: Uuid::new_v4(),
            name: "Direct".to_string(),
        },
    ));

    assert_eq!(handlers.storage().records.len(), 1);
}

#[test]
fn test_list_response_with_pagination() {
    let data = vec![
        TestModel {
            id: Uuid::new_v4(),
            name: "Test1".to_string(),
        },
        TestModel {
            id: Uuid::new_v4(),
            name: "Test2".to_string(),
        },
    ];

    let response = ListResponse::with_pagination(data, 10, 2, 0);
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.total, 10);
    assert_eq!(response.limit, Some(2));
    assert_eq!(response.offset, Some(0));
}

#[test]
fn test_list_response_with_total() {
    let data = vec![TestModel {
        id: Uuid::new_v4(),
        name: "Test".to_string(),
    }];

    let response = ListResponse::with_total(data, 5);
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.total, 5);
    assert_eq!(response.limit, None);
    assert_eq!(response.offset, None);
}

#[test]
fn test_error_debug_format() {
    let error = CrudError::NotFound("test".to_string());
    let debug_string = format!("{:?}", error);
    assert!(debug_string.contains("NotFound"));
}

#[test]
fn test_multiple_error_types() {
    let errors = vec![
        CrudError::NotFound("not found".to_string()),
        CrudError::ValidationError("validation".to_string()),
        CrudError::Conflict("conflict".to_string()),
        CrudError::Internal("internal".to_string()),
    ];

    for error in errors {
        let error_string = format!("{}", error);
        assert!(!error_string.is_empty());
    }
}

#[test]
fn test_handlers_count() {
    let mut storage = TestStorage { records: vec![] };
    
    storage
        .create(CreateTestModel {
            name: "Test1".to_string(),
        })
        .unwrap();
    
    storage
        .create(CreateTestModel {
            name: "Test2".to_string(),
        })
        .unwrap();

    let handlers = CrudHandlers::new(storage);
    assert_eq!(handlers.count().unwrap(), 2);
}

#[test]
fn test_error_scenarios_with_handlers() {
    let storage = TestStorage { records: vec![] };
    let mut handlers = CrudHandlers::new(storage);

    // Create a record
    let created = handlers
        .create(CreateTestModel {
            name: "Test".to_string(),
        })
        .unwrap();

    // Delete it
    handlers.delete(&created.id).unwrap();

    // Try to get deleted record - should error
    let get_result = handlers.get(&created.id);
    assert!(get_result.is_err());

    // Try to update deleted record - should error
    let update_result = handlers.update(
        &created.id,
        UpdateTestModel {
            name: Some("Updated".to_string()),
        },
    );
    assert!(update_result.is_err());

    // Try to delete again - should error
    let delete_result = handlers.delete(&created.id);
    assert!(delete_result.is_err());
}
