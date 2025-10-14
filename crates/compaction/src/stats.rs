use crate::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Collects statistics about database storage
pub struct StatsCollector {
    data_dir: PathBuf,
}

impl StatsCollector {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Collect statistics for a specific model
    pub fn collect_model_stats(&self, model_name: &str) -> Result<ModelStats, String> {
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        let mut stats = ModelStats {
            name: model_name.to_string(),
            total_rows: 0,
            active_rows: 0,
            deleted_rows: 0,
            columns: Vec::new(),
            total_disk_bytes: 0,
            used_bytes: 0,
            dead_bytes: 0,
            dead_space_ratio: 0.0,
            index_sizes: HashMap::new(),
            last_compaction: None,
        };

        // Read tombstone bitmap to determine deleted rows
        let tombstone_path = model_dir.join("tombstones.bin");
        let tombstones = self.read_tombstone_bitmap(&tombstone_path)?;
        stats.total_rows = tombstones.len();
        stats.deleted_rows = tombstones.iter().filter(|&&t| t).count();
        stats.active_rows = stats.total_rows - stats.deleted_rows;

        // Collect fixed column stats
        let fixed_dir = model_dir.join("fixed");
        if fixed_dir.exists() {
            let fixed_columns = self.collect_fixed_column_stats(&fixed_dir, &tombstones)?;
            stats.columns.extend(fixed_columns);
        }

        // Collect variable column stats
        let variable_dir = model_dir.join("variable");
        if variable_dir.exists() {
            let variable_columns =
                self.collect_variable_column_stats(&variable_dir, &tombstones)?;
            stats.columns.extend(variable_columns);
        }

        // Collect index statistics
        stats.index_sizes = self.collect_index_stats(&model_dir)?;

        // Calculate totals
        for column in &stats.columns {
            stats.total_disk_bytes += column.total_bytes;
            stats.used_bytes += column.used_bytes;
            stats.dead_bytes += column.dead_bytes;
        }

        for size in stats.index_sizes.values() {
            stats.total_disk_bytes += size;
            stats.used_bytes += size; // Indexes don't have dead space
        }

        stats.calculate_ratio();

        // Check for last compaction timestamp
        let compaction_marker = model_dir.join(".last_compaction");
        if compaction_marker.exists() {
            if let Ok(content) = fs::read_to_string(&compaction_marker) {
                if let Ok(timestamp) = content.parse::<i64>() {
                    stats.last_compaction = Some(
                        chrono::DateTime::from_timestamp(timestamp, 0)
                            .unwrap_or_else(|| Utc::now()),
                    );
                }
            }
        }

        Ok(stats)
    }

    /// Collect statistics for all models in the database
    pub fn collect_database_stats(&self) -> Result<DatabaseStats, String> {
        let mut db_stats = DatabaseStats {
            models: Vec::new(),
            total_disk_bytes: 0,
            used_bytes: 0,
            dead_bytes: 0,
            dead_space_ratio: 0.0,
            collected_at: Utc::now(),
        };

        // Iterate through all subdirectories (each is a model)
        let entries = fs::read_dir(&self.data_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden directories
                    if !model_name.starts_with('.') {
                        match self.collect_model_stats(model_name) {
                            Ok(model_stats) => {
                                db_stats.total_disk_bytes += model_stats.total_disk_bytes;
                                db_stats.used_bytes += model_stats.used_bytes;
                                db_stats.dead_bytes += model_stats.dead_bytes;
                                db_stats.models.push(model_stats);
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: Failed to collect stats for {}: {}",
                                    model_name, e
                                );
                            }
                        }
                    }
                }
            }
        }

        db_stats.calculate_ratio();

        Ok(db_stats)
    }

    /// Read tombstone bitmap from file
    /// Note: Returns all bits, need to trim based on actual row count
    fn read_tombstone_bitmap(&self, path: &Path) -> Result<Vec<bool>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let mut tombstones = Vec::new();

        for byte in bytes {
            for i in 0..8 {
                tombstones.push((byte & (1 << i)) != 0);
            }
        }

        // Determine actual row count by checking fixed column file size
        // This is a heuristic - in production we'd store row count in manifest
        let model_dir = path.parent().ok_or_else(|| "Invalid path".to_string())?;
        let fixed_dir = model_dir.join("fixed");

        if fixed_dir.exists() {
            if let Ok(entries) = fs::read_dir(&fixed_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file()
                        && entry_path.extension().and_then(|s| s.to_str()) == Some("bin")
                    {
                        if let Ok(metadata) = fs::metadata(&entry_path) {
                            let file_size = metadata.len();
                            // Common fixed sizes: uuid=16, timestamp=8, u64=8, etc.
                            // Try common element sizes
                            for element_size in &[16u64, 8, 4, 1] {
                                if file_size % element_size == 0 {
                                    let row_count = (file_size / element_size) as usize;
                                    if row_count <= tombstones.len() {
                                        tombstones.truncate(row_count);
                                        return Ok(tombstones);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(tombstones)
    }

    /// Collect stats for fixed-size columns
    fn collect_fixed_column_stats(
        &self,
        fixed_dir: &Path,
        tombstones: &[bool],
    ) -> Result<Vec<ColumnStats>, String> {
        let mut columns = Vec::new();

        let entries = fs::read_dir(fixed_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("bin") {
                let file_size = fs::metadata(&path).map_err(|e| e.to_string())?.len();

                let column_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| "Invalid column name".to_string())?
                    .to_string();

                let active_rows = tombstones.iter().filter(|&&t| !t).count();
                let deleted_rows = tombstones.iter().filter(|&&t| t).count();

                // For fixed columns, calculate element size
                let element_size = if tombstones.is_empty() {
                    0
                } else {
                    file_size / tombstones.len() as u64
                };

                let used_bytes = element_size * active_rows as u64;
                let dead_bytes = element_size * deleted_rows as u64;

                let mut col_stats = ColumnStats {
                    name: column_name,
                    column_type: ColumnType::Fixed,
                    total_bytes: file_size,
                    used_bytes,
                    dead_bytes,
                    active_rows,
                    deleted_rows,
                    dead_space_ratio: 0.0,
                };

                col_stats.calculate_ratio();
                columns.push(col_stats);
            }
        }

        Ok(columns)
    }

    /// Collect stats for variable-length columns
    fn collect_variable_column_stats(
        &self,
        variable_dir: &Path,
        tombstones: &[bool],
    ) -> Result<Vec<ColumnStats>, String> {
        let mut columns = Vec::new();

        let entries = fs::read_dir(variable_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            // Look for data files
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.ends_with("_data.bin"))
                    .unwrap_or(false)
            {
                let data_size = fs::metadata(&path).map_err(|e| e.to_string())?.len();

                let column_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_suffix("_data"))
                    .ok_or_else(|| "Invalid column name".to_string())?
                    .to_string();

                // Read offset file to calculate dead space
                let offset_path = variable_dir.join(format!("{}_offsets.bin", column_name));
                let offsets = if offset_path.exists() {
                    self.read_offsets(&offset_path)?
                } else {
                    Vec::new()
                };

                let active_rows = tombstones.iter().filter(|&&t| !t).count();
                let deleted_rows = tombstones.iter().filter(|&&t| t).count();

                // Calculate used vs dead space
                let (used_bytes, dead_bytes) =
                    self.calculate_variable_column_usage(&offsets, tombstones);

                // Total includes offset file
                let offset_size = if offset_path.exists() {
                    fs::metadata(&offset_path).map_err(|e| e.to_string())?.len()
                } else {
                    0
                };

                let total_bytes = data_size + offset_size;

                let mut col_stats = ColumnStats {
                    name: column_name,
                    column_type: ColumnType::Variable,
                    total_bytes,
                    used_bytes,
                    dead_bytes,
                    active_rows,
                    deleted_rows,
                    dead_space_ratio: 0.0,
                };

                col_stats.calculate_ratio();
                columns.push(col_stats);
            }
        }

        Ok(columns)
    }

    /// Read offset file
    fn read_offsets(&self, path: &Path) -> Result<Vec<(u64, u64)>, String> {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let mut offsets = Vec::new();

        for chunk in bytes.chunks_exact(16) {
            let offset = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let length = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
            offsets.push((offset, length));
        }

        Ok(offsets)
    }

    /// Calculate used vs dead bytes for variable column
    fn calculate_variable_column_usage(
        &self,
        offsets: &[(u64, u64)],
        tombstones: &[bool],
    ) -> (u64, u64) {
        let mut used_bytes = 0u64;
        let mut dead_bytes = 0u64;

        for (i, &(_, length)) in offsets.iter().enumerate() {
            if i < tombstones.len() {
                if tombstones[i] {
                    dead_bytes += length;
                } else {
                    used_bytes += length;
                }
            }
        }

        (used_bytes, dead_bytes)
    }

    /// Collect index statistics
    fn collect_index_stats(&self, _model_dir: &Path) -> Result<HashMap<String, u64>, String> {
        let index_sizes = HashMap::new();

        // In-memory indexes don't have disk representation in current implementation
        // This is a placeholder for future index persistence

        // We could estimate based on row count and index type, but for now
        // we'll return empty map since indexes are rebuilt on load

        Ok(index_sizes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_collect_model_stats() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let model_dir = data_dir.join("User");

        // Create model directory structure
        fs::create_dir_all(model_dir.join("fixed")).unwrap();
        fs::create_dir_all(model_dir.join("variable")).unwrap();

        // Create tombstone file (3 rows, 1 deleted)
        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&[0b00000010]).unwrap(); // Second row deleted

        // Create fixed column (uuid, 16 bytes per row)
        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[0u8; 48]).unwrap(); // 3 rows * 16 bytes

        // Create variable column
        let mut email_data = fs::File::create(model_dir.join("variable/email_data.bin")).unwrap();
        email_data
            .write_all(b"alice@example.combob@example.comcharlie@example.com")
            .unwrap();

        let mut email_offsets =
            fs::File::create(model_dir.join("variable/email_offsets.bin")).unwrap();
        // (offset, length) pairs as u64
        email_offsets.write_all(&0u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&15u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&32u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&19u64.to_le_bytes()).unwrap();

        let collector = StatsCollector::new(data_dir);
        let stats = collector.collect_model_stats("User").unwrap();

        assert_eq!(stats.name, "User");
        assert_eq!(stats.total_rows, 3);
        assert_eq!(stats.active_rows, 2);
        assert_eq!(stats.deleted_rows, 1);
        assert_eq!(stats.columns.len(), 2);

        // Check fixed column stats
        let id_col = stats.columns.iter().find(|c| c.name == "id").unwrap();
        assert_eq!(id_col.column_type, ColumnType::Fixed);
        assert_eq!(id_col.total_bytes, 48);
        assert_eq!(id_col.used_bytes, 32); // 2 active rows * 16 bytes
        assert_eq!(id_col.dead_bytes, 16); // 1 deleted row * 16 bytes

        // Check variable column stats
        let email_col = stats.columns.iter().find(|c| c.name == "email").unwrap();
        assert_eq!(email_col.column_type, ColumnType::Variable);
        assert!(email_col.dead_bytes > 0); // Should have dead space from deleted row
    }

    #[test]
    fn test_collect_database_stats() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create two models
        for model_name in &["User", "Post"] {
            let model_dir = data_dir.join(model_name);
            fs::create_dir_all(model_dir.join("fixed")).unwrap();

            let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
            tombstone_file.write_all(&[0u8]).unwrap();

            let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
            id_file.write_all(&[0u8; 16]).unwrap();
        }

        let collector = StatsCollector::new(data_dir);
        let stats = collector.collect_database_stats().unwrap();

        assert_eq!(stats.models.len(), 2);
        assert!(stats.total_disk_bytes > 0);
    }
}
