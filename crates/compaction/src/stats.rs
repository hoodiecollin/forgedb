use crate::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct StatsCollector {
    data_dir: PathBuf,
}

impl StatsCollector {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    fn read_row_count(model_dir: &Path) -> Result<usize, String> {
        let tombstone_path = model_dir.join("tombstones.bin");
        if !tombstone_path.exists() {
            return Err(format!(
                "tombstones.bin not found in {:?}; cannot determine row count",
                model_dir
            ));
        }
        fs::metadata(&tombstone_path)
            .map(|m| m.len() as usize)
            .map_err(|e| format!("Failed to stat {:?}: {}", tombstone_path, e))
    }

    pub fn collect_model_stats(&self, model_name: &str) -> Result<ModelStats, String> {
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        let row_count = Self::read_row_count(&model_dir)?;

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

        let tombstone_path = model_dir.join("tombstones.bin");
        let tombstones = self.read_tombstone_bitmap(&tombstone_path, row_count)?;
        stats.total_rows = tombstones.len();
        stats.deleted_rows = tombstones.iter().filter(|&&t| t).count();
        stats.active_rows = stats.total_rows - stats.deleted_rows;

        let fixed_dir = model_dir.join("fixed");
        if fixed_dir.exists() {
            let fixed_columns = self.collect_fixed_column_stats(&fixed_dir, &tombstones)?;
            stats.columns.extend(fixed_columns);
        }

        let variable_dir = model_dir.join("variable");
        if variable_dir.exists() {
            let variable_columns =
                self.collect_variable_column_stats(&variable_dir, &tombstones)?;
            stats.columns.extend(variable_columns);
        }

        stats.index_sizes = self.collect_index_stats(&model_dir)?;

        for column in &stats.columns {
            stats.total_disk_bytes += column.total_bytes;
            stats.used_bytes += column.used_bytes;
            stats.dead_bytes += column.dead_bytes;
        }

        for size in stats.index_sizes.values() {
            stats.total_disk_bytes += size;
            stats.used_bytes += size;
        }

        stats.calculate_ratio();

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

    pub fn collect_database_stats(&self) -> Result<DatabaseStats, String> {
        let mut db_stats = DatabaseStats {
            models: Vec::new(),
            total_disk_bytes: 0,
            used_bytes: 0,
            dead_bytes: 0,
            dead_space_ratio: 0.0,
            collected_at: Utc::now(),
        };

        let entries = fs::read_dir(&self.data_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !model_name.starts_with('.') {
                        match self.collect_model_stats(model_name) {
                            Ok(model_stats) => {
                                db_stats.total_disk_bytes += model_stats.total_disk_bytes;
                                db_stats.used_bytes += model_stats.used_bytes;
                                db_stats.dead_bytes += model_stats.dead_bytes;
                                db_stats.models.push(model_stats);
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to collect stats for model '{}': {}",
                                    model_name,
                                    e
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

    fn read_tombstone_bitmap(
        &self,
        path: &Path,
        row_count: usize,
    ) -> Result<Vec<bool>, String> {
        if !path.exists() {
            return Ok(vec![false; row_count]);
        }

        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let mut tombstones: Vec<bool> =
            bytes.iter().take(row_count).map(|&b| b != 0).collect();
        tombstones.resize(row_count, false);
        Ok(tombstones)
    }

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

            let name = path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string());
            if path.is_file()
                && name
                    .as_deref()
                    .map(|s| s.ends_with(".bin") && s.contains("_data"))
                    .unwrap_or(false)
            {
                let data_size = fs::metadata(&path).map_err(|e| e.to_string())?.len();

                let offset_path = variable_dir
                    .join(name.as_deref().unwrap().replacen("_data", "_offsets", 1));
                let offsets = if offset_path.exists() {
                    self.read_offsets(&offset_path)?
                } else {
                    Vec::new()
                };

                let active_rows = tombstones.iter().filter(|&&t| !t).count();
                let deleted_rows = tombstones.iter().filter(|&&t| t).count();

                let (used_bytes, dead_bytes) =
                    self.calculate_variable_column_usage(&offsets, tombstones);

                let offset_size = if offset_path.exists() {
                    fs::metadata(&offset_path).map_err(|e| e.to_string())?.len()
                } else {
                    0
                };

                let total_bytes = data_size + offset_size;

                let column_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.replacen("_data", "", 1))
                    .unwrap_or_default();

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

    fn collect_index_stats(&self, _model_dir: &Path) -> Result<HashMap<String, u64>, String> {
        Ok(HashMap::new())
    }
}
