// Persistent UserStorage implementation using columnar storage
// This is a concrete implementation for User { id: +u64, email: &string }

use crate::{ColumnMetadata, ColumnType, Database, FixedColumn, Tombstones, VariableColumn};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: u64,
    pub email: String,
}

pub struct UserStorage {
    db: Database,
    id_column: FixedColumn,
    email_column: VariableColumn,
    tombstones: Tombstones,
    next_id: u64,
    email_index: HashMap<String, usize>,
}

impl UserStorage {
    pub fn new(db_path: PathBuf) -> io::Result<Self> {
        let mut db = Database::open(db_path)?;

        // Initialize columns metadata if first time
        if db.get_manifest().columns.is_empty() {
            let columns = vec![
                ColumnMetadata {
                    name: "id".to_string(),
                    column_type: ColumnType::U64,
                    column_index: 0,
                },
                ColumnMetadata {
                    name: "email".to_string(),
                    column_type: ColumnType::String,
                    column_index: 0,
                },
            ];
            db.set_columns(columns);
            db.save_manifest()?;
        }

        // Open column files
        let id_column = FixedColumn::new(db.fixed_column_path(0), 8)?;
        let email_column =
            VariableColumn::new(db.variable_data_path(0), db.variable_offsets_path(0))?;
        let tombstones = Tombstones::new(db.tombstones_path())?;

        // Calculate next_id from existing data
        let next_id = if id_column.len() > 0 {
            // We'll read the last ID to determine next_id
            // For now, use len + 1 as a simple heuristic
            (id_column.len() as u64) + 1
        } else {
            1
        };

        // Build email index from existing data
        let mut email_index = HashMap::new();
        let mut email_col_mut = email_column;
        for i in 0..email_col_mut.len() {
            let email = email_col_mut.read_string(i)?;
            email_index.insert(email, i);
        }
        let email_column = email_col_mut;

        Ok(UserStorage {
            db,
            id_column,
            email_column,
            tombstones,
            next_id,
            email_index,
        })
    }

    pub fn insert(&mut self, email: String) -> Result<User, String> {
        // Check unique constraint
        if self.email_index.contains_key(&email) {
            return Err("Unique constraint violation: email already exists".to_string());
        }

        let id = self.next_id;
        self.next_id += 1;

        // Write to persistent storage
        self.id_column
            .append_u64(id)
            .map_err(|e| format!("Failed to write id: {}", e))?;

        self.email_column
            .append_string(&email)
            .map_err(|e| format!("Failed to write email: {}", e))?;

        self.tombstones
            .append(false)
            .map_err(|e| format!("Failed to write tombstone: {}", e))?;

        // Update index
        let record_index = self.id_column.len() - 1;
        self.email_index.insert(email.clone(), record_index);

        // Update manifest
        self.db.update_row_count(self.id_column.len());
        self.db
            .save_manifest()
            .map_err(|e| format!("Failed to save manifest: {}", e))?;

        Ok(User { id, email })
    }

    pub fn get(&mut self, id: u64) -> io::Result<Option<User>> {
        // Linear scan to find the record
        // In Sprint 3 we'll add indexing for O(1) lookup
        for i in 0..self.id_column.len() {
            if self.tombstones.is_deleted(i)? {
                continue;
            }

            let record_id = self.id_column.read_u64(i)?;
            if record_id == id {
                let email = self.email_column.read_string(i)?;
                return Ok(Some(User { id, email }));
            }
        }

        Ok(None)
    }

    pub fn list_all(&mut self) -> io::Result<Vec<User>> {
        let mut users = Vec::new();

        for i in 0..self.id_column.len() {
            if self.tombstones.is_deleted(i)? {
                continue;
            }

            let id = self.id_column.read_u64(i)?;
            let email = self.email_column.read_string(i)?;
            users.push(User { id, email });
        }

        Ok(users)
    }

    pub fn len(&self) -> usize {
        self.id_column.len()
    }
}
