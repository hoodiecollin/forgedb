use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Record {
    id: Uuid,
    data: Value,
}

// Mock storage backend that simulates a real storage system
struct MockStorage {
    records: HashMap<Uuid, Record>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }
}

impl CrudOperations for MockStorage {
    type Model = Record;
    type CreateInput = Value;
    type UpdateInput = Value;

    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        Ok(self.records.values().cloned().collect())
    }

    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
        Ok(self.records.get(id).cloned())
    }

    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
        let id = Uuid::new_v4();
        let record = Record {
            id,
            data: input,
        };
        self.records.insert(id, record.clone());
        Ok(record)
    }

    fn update(
        &mut self,
        id: &Uuid,
        input: Self::UpdateInput,
    ) -> CrudResult<Option<Self::Model>> {
        if let Some(record) = self.records.get_mut(id) {
            // Merge updates with existing data
            if let Value::Object(ref mut map) = record.data {
                if let Value::Object(input_map) = input {
                    for (key, value) in input_map {
                        if !value.is_null() {
                            map.insert(key, value);
                        }
                    }
                }
            }
            Ok(Some(record.clone()))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
        Ok(self.records.remove(id).is_some())
    }
}

#[test]
fn test_storage_integration_create() {
    let mut storage = MockStorage::new();
    
    let user_data = serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com"
    });
    
    let record = storage.create(user_data).unwrap();
    assert!(record.data.get("name").is_some());
    assert_eq!(record.data["name"], "Alice");
}

#[test]
fn test_storage_integration_get() {
    let mut storage = MockStorage::new();
    
    let user_data = serde_json::json!({
        "name": "Bob",
        "email": "bob@example.com"
    });
    
    let record = storage.create(user_data).unwrap();
    let id = record.id;
    
    let retrieved = storage.get(&id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, id);
}

#[test]
fn test_storage_integration_list() {
    let mut storage = MockStorage::new();
    
    storage.create(serde_json::json!({"name": "User1"})).unwrap();
    storage.create(serde_json::json!({"name": "User2"})).unwrap();
    storage.create(serde_json::json!({"name": "User3"})).unwrap();
    
    let records = storage.list().unwrap();
    assert_eq!(records.len(), 3);
}

#[test]
fn test_storage_integration_update() {
    let mut storage = MockStorage::new();
    
    let record = storage
        .create(serde_json::json!({
            "name": "Charlie",
            "email": "charlie@example.com"
        }))
        .unwrap();
    
    let updated = storage
        .update(
            &record.id,
            serde_json::json!({
                "name": "Charles"
            }),
        )
        .unwrap();
    
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.data["name"], "Charles");
    assert_eq!(updated.data["email"], "charlie@example.com");
}

#[test]
fn test_storage_integration_delete() {
    let mut storage = MockStorage::new();
    
    let record = storage
        .create(serde_json::json!({
            "name": "ToDelete"
        }))
        .unwrap();
    
    let deleted = storage.delete(&record.id).unwrap();
    assert!(deleted);
    
    let retrieved = storage.get(&record.id).unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_storage_integration_full_cycle() {
    let mut storage = MockStorage::new();
    
    // Create
    let record = storage
        .create(serde_json::json!({
            "name": "Diana",
            "email": "diana@example.com",
            "age": 30
        }))
        .unwrap();
    
    let id = record.id;
    
    // Read
    let retrieved = storage.get(&id).unwrap().unwrap();
    assert_eq!(retrieved.data["name"], "Diana");
    assert_eq!(retrieved.data["age"], 30);
    
    // Update
    let updated = storage
        .update(
            &id,
            serde_json::json!({
                "age": 31
            }),
        )
        .unwrap()
        .unwrap();
    assert_eq!(updated.data["age"], 31);
    assert_eq!(updated.data["name"], "Diana"); // Unchanged
    
    // List
    let all = storage.list().unwrap();
    assert!(all.iter().any(|r| r.id == id));
    
    // Delete
    let deleted = storage.delete(&id).unwrap();
    assert!(deleted);
    
    // Verify
    let after_delete = storage.get(&id).unwrap();
    assert!(after_delete.is_none());
}

#[test]
fn test_storage_with_handlers() {
    let storage = MockStorage::new();
    let mut handlers = CrudHandlers::new(storage);
    
    // Create through handlers
    let record = handlers
        .create(serde_json::json!({
            "name": "Handler Test"
        }))
        .unwrap();
    
    // Get through handlers
    let retrieved = handlers.get(&record.id).unwrap();
    assert_eq!(retrieved.id, record.id);
    
    // List through handlers
    let all = handlers.list().unwrap();
    assert_eq!(all.len(), 1);
    
    // Update through handlers
    let updated = handlers
        .update(
            &record.id,
            serde_json::json!({
                "name": "Updated Handler Test"
            }),
        )
        .unwrap();
    assert_eq!(updated.data["name"], "Updated Handler Test");
    
    // Delete through handlers
    handlers.delete(&record.id).unwrap();
    
    // Verify deletion
    let result = handlers.get(&record.id);
    assert!(result.is_err());
}

#[test]
fn test_storage_count() {
    let mut storage = MockStorage::new();
    
    assert_eq!(storage.count().unwrap(), 0);
    
    storage.create(serde_json::json!({"name": "Test1"})).unwrap();
    assert_eq!(storage.count().unwrap(), 1);
    
    storage.create(serde_json::json!({"name": "Test2"})).unwrap();
    assert_eq!(storage.count().unwrap(), 2);
}

#[test]
fn test_storage_update_nonexistent() {
    let mut storage = MockStorage::new();
    let fake_id = Uuid::new_v4();
    
    let result = storage
        .update(
            &fake_id,
            serde_json::json!({
                "name": "Ghost"
            }),
        )
        .unwrap();
    
    assert!(result.is_none());
}

#[test]
fn test_storage_complex_data() {
    let mut storage = MockStorage::new();
    
    let complex_data = serde_json::json!({
        "name": "Complex User",
        "tags": ["rust", "database", "forgedb"],
        "metadata": {
            "created_at": "2024-01-01",
            "updated_at": "2024-01-02"
        },
        "active": true
    });
    
    let record = storage.create(complex_data).unwrap();
    
    // Verify complex structure is preserved
    assert_eq!(record.data["tags"][0], "rust");
    assert_eq!(record.data["metadata"]["created_at"], "2024-01-01");
    assert_eq!(record.data["active"], true);
    
    // Update nested field
    let updated = storage
        .update(
            &record.id,
            serde_json::json!({
                "metadata": {
                    "updated_at": "2024-01-03"
                }
            }),
        )
        .unwrap()
        .unwrap();
    
    assert_eq!(updated.data["metadata"]["updated_at"], "2024-01-03");
}

#[test]
fn test_storage_operations_sequence() {
    let mut storage = MockStorage::new();
    
    // Create multiple records
    let id1 = storage.create(serde_json::json!({"order": 1})).unwrap().id;
    let id2 = storage.create(serde_json::json!({"order": 2})).unwrap().id;
    let id3 = storage.create(serde_json::json!({"order": 3})).unwrap().id;
    
    assert_eq!(storage.count().unwrap(), 3);
    
    // Delete middle record
    storage.delete(&id2).unwrap();
    assert_eq!(storage.count().unwrap(), 2);
    
    // Verify correct records remain
    let list = storage.list().unwrap();
    assert!(list.iter().any(|r| r.id == id1));
    assert!(list.iter().any(|r| r.id == id3));
    assert!(!list.iter().any(|r| r.id == id2));
}
