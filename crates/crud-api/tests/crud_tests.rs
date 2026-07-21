use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestModel {
    id: Uuid,
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct CreateTestModel {
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct UpdateTestModel {
    name: Option<String>,
    email: Option<String>,
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
            email: input.email,
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
            if let Some(email) = input.email {
                model.email = email;
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
fn test_create_multiple_records() {
    let mut storage = TestStorage { records: vec![] };

    let model1 = storage
        .create(CreateTestModel {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        })
        .unwrap();

    let model2 = storage
        .create(CreateTestModel {
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
        })
        .unwrap();

    assert_eq!(model1.name, "Alice");
    assert_eq!(model2.name, "Bob");
    assert_ne!(model1.id, model2.id);

    let all = storage.list().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_update_partial_fields() {
    let mut storage = TestStorage { records: vec![] };

    let model = storage
        .create(CreateTestModel {
            name: "Charlie".to_string(),
            email: "charlie@example.com".to_string(),
        })
        .unwrap();

    // Update only name
    let updated = storage
        .update(
            &model.id,
            UpdateTestModel {
                name: Some("Charles".to_string()),
                email: None,
            },
        )
        .unwrap();

    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.name, "Charles");
    assert_eq!(updated.email, "charlie@example.com"); // Email unchanged
}

#[test]
fn test_update_nonexistent_record() {
    let mut storage = TestStorage { records: vec![] };
    let fake_id = Uuid::new_v4();

    let result = storage.update(
        &fake_id,
        UpdateTestModel {
            name: Some("Ghost".to_string()),
            email: None,
        },
    );

    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_delete_nonexistent_record() {
    let mut storage = TestStorage { records: vec![] };
    let fake_id = Uuid::new_v4();

    let deleted = storage.delete(&fake_id).unwrap();
    assert!(!deleted);
}

#[test]
fn test_get_nonexistent_record() {
    let storage = TestStorage { records: vec![] };
    let fake_id = Uuid::new_v4();

    let result = storage.get(&fake_id).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_empty_list() {
    let storage = TestStorage { records: vec![] };
    let result = storage.list().unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_count_with_multiple_records() {
    let mut storage = TestStorage { records: vec![] };

    storage
        .create(CreateTestModel {
            name: "User1".to_string(),
            email: "user1@example.com".to_string(),
        })
        .unwrap();
    
    storage
        .create(CreateTestModel {
            name: "User2".to_string(),
            email: "user2@example.com".to_string(),
        })
        .unwrap();
    
    storage
        .create(CreateTestModel {
            name: "User3".to_string(),
            email: "user3@example.com".to_string(),
        })
        .unwrap();

    assert_eq!(storage.count().unwrap(), 3);
}

#[test]
fn test_full_crud_cycle() {
    let mut storage = TestStorage { records: vec![] };

    // Create
    let created = storage
        .create(CreateTestModel {
            name: "Diana".to_string(),
            email: "diana@example.com".to_string(),
        })
        .unwrap();
    let id = created.id;

    // Read - Get
    let retrieved = storage.get(&id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Diana");

    // Read - List
    let all = storage.list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, id);

    // Update
    let updated = storage
        .update(
            &id,
            UpdateTestModel {
                name: Some("Diana Updated".to_string()),
                email: Some("diana.updated@example.com".to_string()),
            },
        )
        .unwrap();
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.name, "Diana Updated");
    assert_eq!(updated.email, "diana.updated@example.com");

    // Delete
    let deleted = storage.delete(&id).unwrap();
    assert!(deleted);

    // Verify deletion
    let after_delete = storage.get(&id).unwrap();
    assert!(after_delete.is_none());

    let all_after = storage.list().unwrap();
    assert_eq!(all_after.len(), 0);
}

#[test]
fn test_multiple_deletes() {
    let mut storage = TestStorage { records: vec![] };

    let model1 = storage
        .create(CreateTestModel {
            name: "Test1".to_string(),
            email: "test1@example.com".to_string(),
        })
        .unwrap();

    let model2 = storage
        .create(CreateTestModel {
            name: "Test2".to_string(),
            email: "test2@example.com".to_string(),
        })
        .unwrap();

    assert_eq!(storage.count().unwrap(), 2);

    // Delete first
    let deleted1 = storage.delete(&model1.id).unwrap();
    assert!(deleted1);
    assert_eq!(storage.count().unwrap(), 1);

    // Delete second
    let deleted2 = storage.delete(&model2.id).unwrap();
    assert!(deleted2);
    assert_eq!(storage.count().unwrap(), 0);

    // Try to delete again
    let deleted_again = storage.delete(&model1.id).unwrap();
    assert!(!deleted_again);
}

#[test]
fn test_update_all_fields() {
    let mut storage = TestStorage { records: vec![] };

    let model = storage
        .create(CreateTestModel {
            name: "Original".to_string(),
            email: "original@example.com".to_string(),
        })
        .unwrap();

    let updated = storage
        .update(
            &model.id,
            UpdateTestModel {
                name: Some("NewName".to_string()),
                email: Some("new@example.com".to_string()),
            },
        )
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "NewName");
    assert_eq!(updated.email, "new@example.com");
}

#[test]
fn test_list_ordering_consistency() {
    let mut storage = TestStorage { records: vec![] };

    let _id1 = storage
        .create(CreateTestModel {
            name: "First".to_string(),
            email: "first@example.com".to_string(),
        })
        .unwrap()
        .id;

    let _id2 = storage
        .create(CreateTestModel {
            name: "Second".to_string(),
            email: "second@example.com".to_string(),
        })
        .unwrap()
        .id;

    let list1 = storage.list().unwrap();
    let list2 = storage.list().unwrap();

    assert_eq!(list1.len(), 2);
    assert_eq!(list2.len(), 2);
    assert_eq!(list1[0].id, list2[0].id);
    assert_eq!(list1[1].id, list2[1].id);
}
