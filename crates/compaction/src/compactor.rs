use crate::stats::StatsCollector;
use crate::types::*;
use chrono::Utc;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

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

    fn read_row_count(model_dir: &Path) -> Result<usize, String> {
        let tombstone_path = model_dir.join("tombstones.bin");
        if !tombstone_path.exists() {
            return Err(format!(
                "tombstones.bin not found in {:?}; cannot determine row count for compaction. \
                 Ensure the model has been initialised (inserted at least once) before compacting.",
                model_dir
            ));
        }
        fs::metadata(&tombstone_path)
            .map(|m| m.len() as usize)
            .map_err(|e| format!("Failed to stat {:?}: {}", tombstone_path, e))
    }

    fn update_manifest_row_count(model_dir: &Path, new_row_count: usize) -> Result<(), String> {
        let manifest_path = model_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest for update: {}", e))?;
        let mut value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse manifest for update: {}", e))?;
        value["row_count"] = serde_json::json!(new_row_count);
        let new_content = serde_json::to_string_pretty(&value)
            .map_err(|e| format!("Failed to serialize updated manifest: {}", e))?;
        let tmp_path = manifest_path.with_extension("json.tmp");
        fs::write(&tmp_path, new_content.as_bytes())
            .map_err(|e| format!("Failed to write manifest temp file: {}", e))?;
        fs::rename(&tmp_path, &manifest_path)
            .map_err(|e| format!("Failed to commit manifest update: {}", e))?;
        Ok(())
    }

    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String> {
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        let row_count = Self::read_row_count(&model_dir)?;

        let tombstone_path = model_dir.join("tombstones.bin");
        let drop_mask = self.read_tombstone_bitmap(&tombstone_path, row_count)?;
        self.compact_with_drop_mask(model_name, &model_dir, drop_mask)
    }

    pub fn compact_model_keeping(
        &self,
        model_name: &str,
        keep: &[usize],
    ) -> Result<CompactionResult, String> {
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        let row_count = Self::read_row_count(&model_dir)?;

        let mut drop_mask = vec![true; row_count];
        for &r in keep {
            if r < row_count {
                drop_mask[r] = false;
            }
        }
        self.compact_with_drop_mask(model_name, &model_dir, drop_mask)
    }

    fn compact_with_drop_mask(
        &self,
        model_name: &str,
        model_dir: &Path,
        tombstones: Vec<bool>,
    ) -> Result<CompactionResult, String> {
        let start_time = Instant::now();

        let collector = StatsCollector::new(&self.data_dir);
        let stats_before = collector.collect_model_stats(model_name)?;
        let bytes_before = stats_before.total_disk_bytes;

        let tombstone_path = model_dir.join("tombstones.bin");

        let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();

        let variable_dir = model_dir.join("variable");
        if variable_dir.exists() {
            let entries = fs::read_dir(&variable_dir).map_err(|e| e.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                let name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if path.is_file() && name.ends_with(".bin") && name.contains("_data") {
                    let offset_name = name.replacen("_data", "_offsets", 1);
                    let offset_path = variable_dir.join(offset_name);
                    if offset_path.exists() {
                        staged.extend(Self::stage_variable_column_write(
                            &path,
                            &offset_path,
                            &tombstones,
                        )?);
                    }
                }
            }
        }

        let fixed_dir = model_dir.join("fixed");
        if fixed_dir.exists() {
            let entries = fs::read_dir(&fixed_dir).map_err(|e| e.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("bin")
                {
                    staged.push(Self::stage_fixed_column_write(&path, &tombstones)?);
                }
            }
        }

        staged.push(Self::stage_tombstones_write(&tombstone_path, &tombstones)?);

        for (tmp, target) in &staged {
            fs::rename(tmp, target).map_err(|e| {
                format!("Failed to commit {:?}: {}", target, e)
            })?;
        }

        let active_count = tombstones.iter().filter(|&&t| !t).count();
        Self::update_manifest_row_count(model_dir, active_count)?;

        let compaction_marker = model_dir.join(".last_compaction");
        fs::write(&compaction_marker, Utc::now().timestamp().to_string())
            .map_err(|e| e.to_string())?;

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

    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String> {
        let entries = fs::read_dir(&self.data_dir).map_err(|e| e.to_string())?;
        let mut results = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|n| n.to_str()) {
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

    pub fn compact_variable_column(
        &self,
        data_path: &Path,
        offset_path: &Path,
        tombstones: &[bool],
    ) -> Result<(), String> {
        let pairs =
            Self::stage_variable_column_write(data_path, offset_path, tombstones)?;
        for (tmp, target) in pairs {
            fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn compact_fixed_column(
        &self,
        column_path: &Path,
        tombstones: &[bool],
    ) -> Result<(), String> {
        if tombstones.is_empty() {
            return Ok(());
        }
        let (tmp, target) = Self::stage_fixed_column_write(column_path, tombstones)?;
        fs::rename(&tmp, &target).map_err(|e| e.to_string())
    }

    fn stage_variable_column_write(
        data_path: &Path,
        offset_path: &Path,
        tombstones: &[bool],
    ) -> Result<Vec<(PathBuf, PathBuf)>, String> {
        let mut data = Vec::new();
        fs::File::open(data_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;

        let offsets = Self::read_offsets_impl(offset_path)?;

        let mut new_data = Vec::new();
        let mut new_offsets: Vec<(u64, u64)> = Vec::new();

        for (i, &(offset, length)) in offsets.iter().enumerate() {
            if i < tombstones.len() && !tombstones[i] {
                let start = offset as usize;
                let end = start + length as usize;
                if end > data.len() {
                    return Err(format!(
                        "Variable column data corruption at row {}: \
                         offset={} + length={} = {} exceeds data file length {}",
                        i, offset, length, end, data.len()
                    ));
                }
                let new_offset = new_data.len() as u64;
                new_data.extend_from_slice(&data[start..end]);
                new_offsets.push((new_offset, length));
            }
        }

        let temp_data_path = data_path.with_extension("bin.tmp");
        let mut data_file =
            fs::File::create(&temp_data_path).map_err(|e| e.to_string())?;
        data_file.write_all(&new_data).map_err(|e| e.to_string())?;
        data_file.sync_all().map_err(|e| e.to_string())?;
        drop(data_file);

        let temp_offset_path = offset_path.with_extension("bin.tmp");
        let mut offset_file =
            fs::File::create(&temp_offset_path).map_err(|e| e.to_string())?;
        for (off, len) in new_offsets {
            offset_file
                .write_all(&off.to_le_bytes())
                .map_err(|e| e.to_string())?;
            offset_file
                .write_all(&len.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        offset_file.sync_all().map_err(|e| e.to_string())?;
        drop(offset_file);

        Ok(vec![
            (temp_data_path, data_path.to_path_buf()),
            (temp_offset_path, offset_path.to_path_buf()),
        ])
    }

    fn stage_fixed_column_write(
        column_path: &Path,
        tombstones: &[bool],
    ) -> Result<(PathBuf, PathBuf), String> {
        let mut data = Vec::new();
        fs::File::open(column_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;

        let element_size = if tombstones.is_empty() { 0 } else { data.len() / tombstones.len() };

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

        let temp_path = column_path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(&new_data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        Ok((temp_path, column_path.to_path_buf()))
    }

    fn stage_tombstones_write(
        tombstone_path: &Path,
        tombstones: &[bool],
    ) -> Result<(PathBuf, PathBuf), String> {
        let active_count = tombstones.iter().filter(|&&t| !t).count();
        let new_bitmap = vec![0u8; active_count];

        let temp_path = tombstone_path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(&new_bitmap).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        Ok((temp_path, tombstone_path.to_path_buf()))
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

    fn read_offsets_impl(path: &Path) -> Result<Vec<(u64, u64)>, String> {
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
