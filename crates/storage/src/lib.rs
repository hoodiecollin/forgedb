// Sprint 2: Storage Persistence Implementation

pub mod user_storage;
pub use user_storage::{User, UserStorage};

// Sprint 7: WAL re-exports
pub use forgedb_wal::{
    FsyncPolicy, Transaction, TransactionId, WalEntry, WalManager, WalOperation, WalValue,
};

// Sprint 2: Storage Persistence Implementation
//
// Architecture:
// - Fixed-size columns: Memory-mapped files (fixed/u64_0.bin, fixed/u64_1.bin, etc.)
// - Variable-length columns: Append-only data file + offset index (variable/string_data.bin + variable/string_offsets.bin)
// - Metadata: manifest.json (stores schema info, column mappings, row count, etc.)
// - Tombstones: Bitmap for deleted records (tombstones.bin)
//
// Layout example for User { id: +u64, email: &string }:
//   data/
//     manifest.json          # Metadata
//     tombstones.bin         # Deletion bitmap
//     fixed/
//       u64_0.bin           # id column (8 bytes per record)
//     variable/
//       string_data_0.bin   # email strings (append-only)
//       string_offsets_0.bin # (offset, length) pairs (16 bytes per record)

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Manifest stores metadata about the database
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub row_count: usize,
    pub columns: Vec<ColumnMetadata>,
    #[serde(default)]
    pub wal_enabled: bool,
    #[serde(default)]
    pub last_checkpoint: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub column_type: ColumnType,
    pub column_index: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColumnType {
    U64,
    String,
}

/// Fixed-size column storage using memory-mapped files
pub struct FixedColumn {
    file: File,
    row_count: usize,
    value_size: usize, // bytes per value
}

impl FixedColumn {
    pub fn new(path: PathBuf, value_size: usize) -> io::Result<Self> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        let row_count = file.metadata()?.len() as usize / value_size;

        Ok(FixedColumn {
            file,
            row_count,
            value_size,
        })
    }

    pub fn append_u64(&mut self, value: u64) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.file.sync_all()?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_u64(&mut self, index: usize) -> io::Result<u64> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        self.file.seek(SeekFrom::Start(offset))?;

        let mut buf = [0u8; 8];
        self.file.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn len(&self) -> usize {
        self.row_count
    }
}

/// Variable-length column storage (for strings)
/// Stores data in append-only file + separate offset index
pub struct VariableColumn {
    data_file: File,
    offsets_file: File,
    row_count: usize,
    current_data_offset: u64,
}

impl VariableColumn {
    pub fn new(data_path: PathBuf, offsets_path: PathBuf) -> io::Result<Self> {
        // Create parent directories
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&data_path)?;

        let offsets_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&offsets_path)?;

        let current_data_offset = data_file.metadata()?.len();
        let row_count = offsets_file.metadata()?.len() as usize / 16; // 16 bytes per (offset, length)

        Ok(VariableColumn {
            data_file,
            offsets_file,
            row_count,
            current_data_offset,
        })
    }

    pub fn append_string(&mut self, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        let length = bytes.len() as u64;

        // Write data to data file
        self.data_file.seek(SeekFrom::End(0))?;
        self.data_file.write_all(bytes)?;
        self.data_file.sync_all()?;

        // Write (offset, length) to offsets file
        self.offsets_file.seek(SeekFrom::End(0))?;
        self.offsets_file
            .write_all(&self.current_data_offset.to_le_bytes())?;
        self.offsets_file.write_all(&length.to_le_bytes())?;
        self.offsets_file.sync_all()?;

        self.current_data_offset += length;
        self.row_count += 1;

        Ok(())
    }

    pub fn read_string(&mut self, index: usize) -> io::Result<String> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        // Read offset and length from offsets file
        let offsets_pos = (index * 16) as u64;
        self.offsets_file.seek(SeekFrom::Start(offsets_pos))?;

        let mut offset_buf = [0u8; 8];
        let mut length_buf = [0u8; 8];
        self.offsets_file.read_exact(&mut offset_buf)?;
        self.offsets_file.read_exact(&mut length_buf)?;

        let offset = u64::from_le_bytes(offset_buf);
        let length = u64::from_le_bytes(length_buf);

        // Read data from data file
        self.data_file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; length as usize];
        self.data_file.read_exact(&mut data)?;

        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn len(&self) -> usize {
        self.row_count
    }
}

/// Tombstone bitmap for tracking deleted records
pub struct Tombstones {
    file: File,
    count: usize,
}

impl Tombstones {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        let count = file.metadata()?.len() as usize;

        Ok(Tombstones { file, count })
    }

    pub fn append(&mut self, deleted: bool) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&[if deleted { 1u8 } else { 0u8 }])?;
        self.file.sync_all()?;
        self.count += 1;
        Ok(())
    }

    pub fn is_deleted(&mut self, index: usize) -> io::Result<bool> {
        if index >= self.count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        self.file.seek(SeekFrom::Start(index as u64))?;
        let mut buf = [0u8; 1];
        self.file.read_exact(&mut buf)?;
        Ok(buf[0] != 0)
    }

    pub fn len(&self) -> usize {
        self.count
    }
}

/// Database directory manager
pub struct Database {
    root_path: PathBuf,
    manifest: Manifest,
    wal_manager: Option<forgedb_wal::WalManager>,
}

impl Database {
    pub fn open(root_path: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root_path)?;

        let manifest_path = root_path.join("manifest.json");
        let manifest = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // Create default manifest
            Manifest {
                schema_version: 1,
                row_count: 0,
                columns: Vec::new(),
                wal_enabled: false,
                last_checkpoint: 0,
            }
        };

        Ok(Database {
            root_path,
            manifest,
            wal_manager: None,
        })
    }

    /// Open database with WAL support
    pub fn open_with_wal(
        root_path: PathBuf,
        fsync_policy: forgedb_wal::FsyncPolicy,
    ) -> io::Result<Self> {
        fs::create_dir_all(&root_path)?;

        let manifest_path = root_path.join("manifest.json");
        let mut manifest = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // Create default manifest
            Manifest {
                schema_version: 1,
                row_count: 0,
                columns: Vec::new(),
                wal_enabled: true,
                last_checkpoint: 0,
            }
        };

        // Mark WAL as enabled
        manifest.wal_enabled = true;

        // Open WAL
        let wal_path = root_path.join("wal.log");
        let wal_manager = forgedb_wal::WalManager::open(wal_path, fsync_policy)?;

        Ok(Database {
            root_path,
            manifest,
            wal_manager: Some(wal_manager),
        })
    }

    /// Get a mutable reference to the WAL manager
    pub fn wal_mut(&mut self) -> Option<&mut forgedb_wal::WalManager> {
        self.wal_manager.as_mut()
    }

    /// Get a reference to the WAL manager
    pub fn wal(&self) -> Option<&forgedb_wal::WalManager> {
        self.wal_manager.as_ref()
    }

    /// Check if WAL is enabled
    pub fn has_wal(&self) -> bool {
        self.wal_manager.is_some()
    }

    pub fn save_manifest(&self) -> io::Result<()> {
        let manifest_path = self.root_path.join("manifest.json");
        let content = serde_json::to_string_pretty(&self.manifest)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    pub fn get_manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn update_row_count(&mut self, count: usize) {
        self.manifest.row_count = count;
    }

    pub fn set_columns(&mut self, columns: Vec<ColumnMetadata>) {
        self.manifest.columns = columns;
    }

    pub fn fixed_column_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("fixed/u64_{}.bin", column_index))
    }

    pub fn variable_data_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("variable/string_data_{}.bin", column_index))
    }

    pub fn variable_offsets_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("variable/string_offsets_{}.bin", column_index))
    }

    pub fn tombstones_path(&self) -> PathBuf {
        self.root_path.join("tombstones.bin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_fixed_column_u64() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_fixed");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("test_u64.bin");
        let mut col = FixedColumn::new(path, 8).unwrap();

        // Append values
        col.append_u64(42).unwrap();
        col.append_u64(100).unwrap();
        col.append_u64(999).unwrap();

        // Read values
        assert_eq!(col.read_u64(0).unwrap(), 42);
        assert_eq!(col.read_u64(1).unwrap(), 100);
        assert_eq!(col.read_u64(2).unwrap(), 999);
        assert_eq!(col.len(), 3);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_variable_column_string() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_variable");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let data_path = temp_dir.join("test_data.bin");
        let offsets_path = temp_dir.join("test_offsets.bin");
        let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

        // Append strings
        col.append_string("hello").unwrap();
        col.append_string("world").unwrap();
        col.append_string("test@example.com").unwrap();

        // Read strings
        assert_eq!(col.read_string(0).unwrap(), "hello");
        assert_eq!(col.read_string(1).unwrap(), "world");
        assert_eq!(col.read_string(2).unwrap(), "test@example.com");
        assert_eq!(col.len(), 3);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_tombstones() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_tombstones");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("tombstones.bin");
        let mut tombstones = Tombstones::new(path).unwrap();

        tombstones.append(false).unwrap();
        tombstones.append(true).unwrap();
        tombstones.append(false).unwrap();

        assert_eq!(tombstones.is_deleted(0).unwrap(), false);
        assert_eq!(tombstones.is_deleted(1).unwrap(), true);
        assert_eq!(tombstones.is_deleted(2).unwrap(), false);
        assert_eq!(tombstones.len(), 3);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_database_manifest() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_manifest");
        let _ = fs::remove_dir_all(&temp_dir);

        let mut db = Database::open(temp_dir.clone()).unwrap();

        // Set up columns
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
        db.update_row_count(42);
        db.save_manifest().unwrap();

        // Reopen and verify
        let db2 = Database::open(temp_dir.clone()).unwrap();
        assert_eq!(db2.get_manifest().row_count, 42);
        assert_eq!(db2.get_manifest().columns.len(), 2);
        assert_eq!(db2.get_manifest().columns[0].name, "id");
        assert_eq!(db2.get_manifest().columns[1].name, "email");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_fixed_column_out_of_bounds() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_oob");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("test_u64.bin");
        let mut col = FixedColumn::new(path, 8).unwrap();

        col.append_u64(42).unwrap();

        // Reading beyond bounds should error
        let result = col.read_u64(1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_fixed_column_persistence() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_fixed_persist");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("test_u64.bin");

        // Write data
        {
            let mut col = FixedColumn::new(path.clone(), 8).unwrap();
            col.append_u64(100).unwrap();
            col.append_u64(200).unwrap();
            col.append_u64(300).unwrap();
        }

        // Reopen and verify
        {
            let mut col = FixedColumn::new(path.clone(), 8).unwrap();
            assert_eq!(col.len(), 3);
            assert_eq!(col.read_u64(0).unwrap(), 100);
            assert_eq!(col.read_u64(1).unwrap(), 200);
            assert_eq!(col.read_u64(2).unwrap(), 300);
        }

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_variable_column_empty_string() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_empty_string");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let data_path = temp_dir.join("test_data.bin");
        let offsets_path = temp_dir.join("test_offsets.bin");
        let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

        col.append_string("").unwrap();
        col.append_string("hello").unwrap();
        col.append_string("").unwrap();

        assert_eq!(col.read_string(0).unwrap(), "");
        assert_eq!(col.read_string(1).unwrap(), "hello");
        assert_eq!(col.read_string(2).unwrap(), "");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_variable_column_large_string() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_large_string");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let data_path = temp_dir.join("test_data.bin");
        let offsets_path = temp_dir.join("test_offsets.bin");
        let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

        // Test with 1KB string
        let large_string = "x".repeat(1024);
        col.append_string(&large_string).unwrap();
        col.append_string("small").unwrap();

        assert_eq!(col.read_string(0).unwrap(), large_string);
        assert_eq!(col.read_string(1).unwrap(), "small");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_variable_column_persistence() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_var_persist");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let data_path = temp_dir.join("test_data.bin");
        let offsets_path = temp_dir.join("test_offsets.bin");

        // Write data
        {
            let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
            col.append_string("first").unwrap();
            col.append_string("second").unwrap();
            col.append_string("third").unwrap();
        }

        // Reopen and verify
        {
            let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
            assert_eq!(col.len(), 3);
            assert_eq!(col.read_string(0).unwrap(), "first");
            assert_eq!(col.read_string(1).unwrap(), "second");
            assert_eq!(col.read_string(2).unwrap(), "third");

            // Append more after reopening
            col.append_string("fourth").unwrap();
        }

        // Verify new data persisted
        {
            let mut col = VariableColumn::new(data_path, offsets_path).unwrap();
            assert_eq!(col.len(), 4);
            assert_eq!(col.read_string(3).unwrap(), "fourth");
        }

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_variable_column_out_of_bounds() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_var_oob");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let data_path = temp_dir.join("test_data.bin");
        let offsets_path = temp_dir.join("test_offsets.bin");
        let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

        col.append_string("test").unwrap();

        // Reading beyond bounds should error
        let result = col.read_string(1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_tombstones_out_of_bounds() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_tomb_oob");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("tombstones.bin");
        let mut tombstones = Tombstones::new(path).unwrap();

        tombstones.append(false).unwrap();

        // Reading beyond bounds should error
        let result = tombstones.is_deleted(1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_database_empty() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_empty");
        let _ = fs::remove_dir_all(&temp_dir);

        let db = Database::open(temp_dir.clone()).unwrap();

        // Empty database should have default manifest
        assert_eq!(db.get_manifest().row_count, 0);
        assert_eq!(db.get_manifest().columns.len(), 0);
        assert_eq!(db.get_manifest().schema_version, 1);

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
