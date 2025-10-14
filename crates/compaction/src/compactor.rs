use crate::stats::StatsCollector;
use crate::types::*;
use chrono::Utc;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Performs compaction operations on database storage
pub struct Compactor {
    data_dir: PathBuf,
    config: CompactionConfig,
}

impl Compactor {
    pub fn new<P: AsRef<Path>>(data_dir: P, config: CompactionConfig) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            config,
        }
    }

    /// Compact a specific model
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String> {
        let start_time = Instant::now();
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        // Collect stats before compaction
        let collector = StatsCollector::new(&self.data_dir);
        let stats_before = collector.collect_model_stats(model_name)?;
        let bytes_before = stats_before.total_disk_bytes;

        // Read tombstone bitmap
        let tombstone_path = model_dir.join("tombstones.bin");
        let tombstones = self.read_tombstone_bitmap(&tombstone_path)?;

        // Compact variable columns
        let variable_dir = model_dir.join("variable");
        if variable_dir.exists() {
            self.compact_variable_columns(&variable_dir, &tombstones)?;
        }

        // Compact fixed columns
        let fixed_dir = model_dir.join("fixed");
        if fixed_dir.exists() {
            self.compact_fixed_columns(&fixed_dir, &tombstones)?;
        }

        // Update tombstone bitmap (remove deleted entries)
        self.compact_tombstones(&tombstone_path, &tombstones)?;

        // Record compaction timestamp
        let compaction_marker = model_dir.join(".last_compaction");
        fs::write(&compaction_marker, Utc::now().timestamp().to_string())
            .map_err(|e| e.to_string())?;

        // Collect stats after compaction
        let stats_after = collector.collect_model_stats(model_name)?;
        let bytes_after = stats_after.total_disk_bytes;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(CompactionResult {
            model_name: model_name.to_string(),
            bytes_before,
            bytes_after,
            bytes_reclaimed: bytes_before.saturating_sub(bytes_after),
            duration_ms,
            completed_at: Utc::now(),
            success: true,
            error: None,
        })
    }

    /// Compact all models in the database
    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String> {
        let entries = fs::read_dir(&self.data_dir).map_err(|e| e.to_string())?;
        let mut results = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden directories
                    if !model_name.starts_with('.') {
                        match self.compact_model(model_name) {
                            Ok(result) => results.push(result),
                            Err(e) => {
                                results.push(CompactionResult {
                                    model_name: model_name.to_string(),
                                    bytes_before: 0,
                                    bytes_after: 0,
                                    bytes_reclaimed: 0,
                                    duration_ms: 0,
                                    completed_at: Utc::now(),
                                    success: false,
                                    error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Compact only models that exceed the dead space threshold
    pub fn compact_needed(&self) -> Result<Vec<CompactionResult>, String> {
        let collector = StatsCollector::new(&self.data_dir);
        let db_stats = collector.collect_database_stats()?;

        let models_to_compact = db_stats.models_needing_compaction(&self.config);
        let mut results = Vec::new();

        for model_stats in models_to_compact {
            match self.compact_model(&model_stats.name) {
                Ok(result) => results.push(result),
                Err(e) => {
                    results.push(CompactionResult {
                        model_name: model_stats.name.clone(),
                        bytes_before: 0,
                        bytes_after: 0,
                        bytes_reclaimed: 0,
                        duration_ms: 0,
                        completed_at: Utc::now(),
                        success: false,
                        error: Some(e),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Compact variable-length columns
    fn compact_variable_columns(
        &self,
        variable_dir: &Path,
        tombstones: &[bool],
    ) -> Result<(), String> {
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
                let column_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_suffix("_data"))
                    .ok_or_else(|| "Invalid column name".to_string())?
                    .to_string();

                let data_path = path.clone();
                let offset_path = variable_dir.join(format!("{}_offsets.bin", column_name));

                if offset_path.exists() {
                    self.compact_variable_column(&data_path, &offset_path, tombstones)?;
                }
            }
        }

        Ok(())
    }

    /// Compact a single variable column
    fn compact_variable_column(
        &self,
        data_path: &Path,
        offset_path: &Path,
        tombstones: &[bool],
    ) -> Result<(), String> {
        // Read existing data
        let mut data = Vec::new();
        fs::File::open(data_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;

        // Read offsets
        let offsets = self.read_offsets(offset_path)?;

        // Build compacted data and new offsets
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::new();

        for (i, &(offset, length)) in offsets.iter().enumerate() {
            if i < tombstones.len() && !tombstones[i] {
                // Active row - copy data
                let start = offset as usize;
                let end = start + length as usize;
                if end <= data.len() {
                    let new_offset = new_data.len() as u64;
                    new_data.extend_from_slice(&data[start..end]);
                    new_offsets.push((new_offset, length));
                }
            }
        }

        // Write compacted data
        let temp_data_path = data_path.with_extension("bin.tmp");
        let mut data_file =
            fs::File::create(&temp_data_path).map_err(|e| e.to_string())?;
        data_file.write_all(&new_data).map_err(|e| e.to_string())?;
        data_file.sync_all().map_err(|e| e.to_string())?;
        drop(data_file);

        // Write compacted offsets
        let temp_offset_path = offset_path.with_extension("bin.tmp");
        let mut offset_file =
            fs::File::create(&temp_offset_path).map_err(|e| e.to_string())?;
        for (offset, length) in new_offsets {
            offset_file
                .write_all(&offset.to_le_bytes())
                .map_err(|e| e.to_string())?;
            offset_file
                .write_all(&length.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        offset_file.sync_all().map_err(|e| e.to_string())?;
        drop(offset_file);

        // Atomic replace
        fs::rename(&temp_data_path, data_path).map_err(|e| e.to_string())?;
        fs::rename(&temp_offset_path, offset_path).map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Compact fixed-size columns
    fn compact_fixed_columns(&self, fixed_dir: &Path, tombstones: &[bool]) -> Result<(), String> {
        let entries = fs::read_dir(fixed_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("bin") {
                self.compact_fixed_column(&path, tombstones)?;
            }
        }

        Ok(())
    }

    /// Compact a single fixed-size column
    fn compact_fixed_column(&self, column_path: &Path, tombstones: &[bool]) -> Result<(), String> {
        // Read existing data
        let mut data = Vec::new();
        fs::File::open(column_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;

        if tombstones.is_empty() {
            return Ok(());
        }

        // Calculate element size
        let element_size = data.len() / tombstones.len();

        // Build compacted data
        let mut new_data = Vec::new();
        for (i, &is_deleted) in tombstones.iter().enumerate() {
            if !is_deleted {
                let start = i * element_size;
                let end = start + element_size;
                if end <= data.len() {
                    new_data.extend_from_slice(&data[start..end]);
                }
            }
        }

        // Write compacted data
        let temp_path = column_path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(&new_data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        // Atomic replace
        fs::rename(&temp_path, column_path).map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Compact tombstone bitmap (remove deleted entries)
    fn compact_tombstones(&self, tombstone_path: &Path, tombstones: &[bool]) -> Result<(), String> {
        // Count active rows
        let active_count = tombstones.iter().filter(|&&t| !t).count();

        // Create new bitmap with all false (no deleted rows after compaction)
        let new_bitmap_size = (active_count + 7) / 8;
        let new_bitmap = vec![0u8; new_bitmap_size];

        // Write new bitmap
        let temp_path = tombstone_path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(&new_bitmap).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        // Atomic replace
        fs::rename(&temp_path, tombstone_path).map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Read tombstone bitmap from file
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
        let model_dir = path.parent().ok_or_else(|| "Invalid path".to_string())?;
        let fixed_dir = model_dir.join("fixed");

        if fixed_dir.exists() {
            if let Ok(entries) = fs::read_dir(&fixed_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() && entry_path.extension().and_then(|s| s.to_str()) == Some("bin") {
                        if let Ok(metadata) = fs::metadata(&entry_path) {
                            let file_size = metadata.len();
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_compact_variable_column() {
        let temp_dir = TempDir::new().unwrap();
        let variable_dir = temp_dir.path();

        // Create test data with 3 rows, middle row deleted
        let data_path = variable_dir.join("email_data.bin");
        let offset_path = variable_dir.join("email_offsets.bin");

        let mut data_file = fs::File::create(&data_path).unwrap();
        data_file
            .write_all(b"alice@example.combob@example.comcharlie@example.com")
            .unwrap();

        let mut offset_file = fs::File::create(&offset_path).unwrap();
        offset_file.write_all(&0u64.to_le_bytes()).unwrap();
        offset_file.write_all(&17u64.to_le_bytes()).unwrap();
        offset_file.write_all(&17u64.to_le_bytes()).unwrap();
        offset_file.write_all(&15u64.to_le_bytes()).unwrap();
        offset_file.write_all(&32u64.to_le_bytes()).unwrap();
        offset_file.write_all(&19u64.to_le_bytes()).unwrap();

        let tombstones = vec![false, true, false]; // Middle row deleted

        let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
        compactor
            .compact_variable_column(&data_path, &offset_path, &tombstones)
            .unwrap();

        // Read compacted data
        let compacted_data = fs::read(&data_path).unwrap();
        let expected = b"alice@example.comcharlie@example.com";
        assert_eq!(compacted_data, expected);

        // Read compacted offsets
        let offset_bytes = fs::read(&offset_path).unwrap();
        assert_eq!(offset_bytes.len(), 32); // 2 entries * 16 bytes

        let offset1 = u64::from_le_bytes(offset_bytes[0..8].try_into().unwrap());
        let length1 = u64::from_le_bytes(offset_bytes[8..16].try_into().unwrap());
        assert_eq!(offset1, 0);
        assert_eq!(length1, 17);

        let offset2 = u64::from_le_bytes(offset_bytes[16..24].try_into().unwrap());
        let length2 = u64::from_le_bytes(offset_bytes[24..32].try_into().unwrap());
        assert_eq!(offset2, 17);
        assert_eq!(length2, 19);
    }

    #[test]
    fn test_compact_fixed_column() {
        let temp_dir = TempDir::new().unwrap();
        let column_path = temp_dir.path().join("id.bin");

        // Create test data with 3 UUIDs (16 bytes each), middle row deleted
        let mut file = fs::File::create(&column_path).unwrap();
        file.write_all(&[1u8; 16]).unwrap(); // UUID 1
        file.write_all(&[2u8; 16]).unwrap(); // UUID 2 (deleted)
        file.write_all(&[3u8; 16]).unwrap(); // UUID 3

        let tombstones = vec![false, true, false]; // Middle row deleted

        let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
        compactor
            .compact_fixed_column(&column_path, &tombstones)
            .unwrap();

        // Read compacted data
        let compacted_data = fs::read(&column_path).unwrap();
        assert_eq!(compacted_data.len(), 32); // 2 UUIDs * 16 bytes

        assert_eq!(&compacted_data[0..16], &[1u8; 16]);
        assert_eq!(&compacted_data[16..32], &[3u8; 16]);
    }

    #[test]
    fn test_compact_model() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let model_dir = data_dir.join("User");

        // Create model directory structure
        fs::create_dir_all(model_dir.join("fixed")).unwrap();
        fs::create_dir_all(model_dir.join("variable")).unwrap();

        // Create tombstone file (3 rows, middle row deleted)
        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&[0b00000010]).unwrap(); // Second bit set

        // Create fixed column
        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[1u8; 16]).unwrap();
        id_file.write_all(&[2u8; 16]).unwrap();
        id_file.write_all(&[3u8; 16]).unwrap();

        // Create variable column
        let mut email_data = fs::File::create(model_dir.join("variable/email_data.bin")).unwrap();
        email_data
            .write_all(b"alice@example.combob@example.comcharlie@example.com")
            .unwrap();

        let mut email_offsets =
            fs::File::create(model_dir.join("variable/email_offsets.bin")).unwrap();
        email_offsets.write_all(&0u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&15u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&32u64.to_le_bytes()).unwrap();
        email_offsets.write_all(&19u64.to_le_bytes()).unwrap();

        let compactor = Compactor::new(data_dir, CompactionConfig::default());
        let result = compactor.compact_model("User").unwrap();

        assert!(result.success);
        assert!(result.bytes_reclaimed > 0);
        assert_eq!(result.model_name, "User");

        // Verify compacted data
        let id_data = fs::read(model_dir.join("fixed/id.bin")).unwrap();
        assert_eq!(id_data.len(), 32); // 2 rows * 16 bytes

        let email_data = fs::read(model_dir.join("variable/email_data.bin")).unwrap();
        assert_eq!(email_data, b"alice@example.comcharlie@example.com");

        // Verify tombstone bitmap reset
        let tombstones = fs::read(model_dir.join("tombstones.bin")).unwrap();
        assert_eq!(tombstones, vec![0u8]); // All clear (2 active rows fit in 1 byte)
    }

    #[test]
    fn test_compact_reclaims_space() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let model_dir = data_dir.join("User");

        fs::create_dir_all(model_dir.join("fixed")).unwrap();

        // Create 10 rows with 5 deleted (50% dead space)
        let mut tombstone_bytes = vec![0u8; 2]; // 10 rows needs 2 bytes
        tombstone_bytes[0] = 0b01010101; // Alternating pattern
        tombstone_bytes[1] = 0b00000001; // One more deleted

        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&tombstone_bytes).unwrap();

        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[0u8; 160]).unwrap(); // 10 rows * 16 bytes

        let collector = StatsCollector::new(data_dir);
        let _stats_before = collector.collect_model_stats("User").unwrap();

        let compactor = Compactor::new(data_dir, CompactionConfig::default());
        let result = compactor.compact_model("User").unwrap();

        let stats_after = collector.collect_model_stats("User").unwrap();

        // Should reclaim ~50% of space
        assert!(result.bytes_reclaimed > 0);
        let reclaim_pct = result.reclaim_percentage();
        assert!(reclaim_pct > 40.0 && reclaim_pct < 60.0);

        // Active rows should be preserved
        assert_eq!(stats_after.active_rows, 5);
        assert_eq!(stats_after.deleted_rows, 0);
    }
}
