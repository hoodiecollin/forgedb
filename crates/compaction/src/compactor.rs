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

    /// Read the authoritative row count from a model's `manifest.json`.
    ///
    /// Compaction must not proceed without this — guessing element sizes from
    /// the first fixed column file is unreliable and corrupts columns with
    /// different widths (C1 fix).
    fn read_manifest_row_count(model_dir: &Path) -> Result<usize, String> {
        let manifest_path = model_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(format!(
                "manifest.json not found in {:?}; cannot determine row count for compaction. \
                 Ensure the model has been initialised via the storage layer before compacting.",
                model_dir
            ));
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest {:?}: {}", manifest_path, e))?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse manifest {:?}: {}", manifest_path, e))?;
        value["row_count"]
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| {
                format!(
                    "manifest.json at {:?} is missing the 'row_count' field",
                    manifest_path
                )
            })
    }

    /// Write a new `row_count` back to `manifest.json` after compaction.
    ///
    /// Preserves all other fields.  Written via a temp file + rename so a crash
    /// mid-write does not corrupt the manifest.
    fn update_manifest_row_count(model_dir: &Path, new_row_count: usize) -> Result<(), String> {
        let manifest_path = model_dir.join("manifest.json");
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

    /// Compact a specific model.
    ///
    /// # Crash safety (C2)
    ///
    /// All compacted column files are written as `.tmp` siblings first.  Only
    /// after every write succeeds are they renamed to their final paths.  The
    /// manifest `row_count` is updated last so it acts as a logical commit
    /// record: if the process dies before the manifest rename, the column files
    /// are already consistent and a re-run is idempotent.
    ///
    /// **Residual window**: individual file renames are atomic but not grouped
    /// — a crash between two column-file renames leaves those columns at
    /// different post-compaction versions.  Full directory-level atomicity is
    /// deferred.
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String> {
        let start_time = Instant::now();
        let model_dir = self.data_dir.join(model_name);

        if !model_dir.exists() {
            return Err(format!("Model directory not found: {:?}", model_dir));
        }

        // --- C1: read row count from manifest; no guessing ---
        let row_count = Self::read_manifest_row_count(&model_dir)?;

        // Collect stats before compaction (needs manifest row_count too)
        let collector = StatsCollector::new(&self.data_dir);
        let stats_before = collector.collect_model_stats(model_name)?;
        let bytes_before = stats_before.total_disk_bytes;

        // Read tombstone bitmap using the authoritative row count
        let tombstone_path = model_dir.join("tombstones.bin");
        let tombstones = self.read_tombstone_bitmap(&tombstone_path, row_count)?;

        // --- C2: stage all writes before committing any ---
        let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();

        // Stage variable columns
        let variable_dir = model_dir.join("variable");
        if variable_dir.exists() {
            let entries = fs::read_dir(&variable_dir).map_err(|e| e.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
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
                        .ok_or_else(|| "Invalid column file name".to_string())?
                        .to_string();
                    let data_path = path.clone();
                    let offset_path =
                        variable_dir.join(format!("{}_offsets.bin", column_name));
                    if offset_path.exists() {
                        staged.extend(Self::stage_variable_column_write(
                            &data_path,
                            &offset_path,
                            &tombstones,
                        )?);
                    }
                }
            }
        }

        // Stage fixed columns
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

        // Stage tombstone bitmap
        staged.push(Self::stage_tombstones_write(&tombstone_path, &tombstones)?);

        // --- Commit: rename all staged temp files ---
        // Residual window documented on this function.
        for (tmp, target) in &staged {
            fs::rename(tmp, target).map_err(|e| {
                format!("Failed to commit {:?}: {}", target, e)
            })?;
        }

        // Update manifest row_count to the post-compaction active count.
        // This is the logical commit record (last rename so it confirms all column
        // renames above have completed).
        let active_count = tombstones.iter().filter(|&&t| !t).count();
        Self::update_manifest_row_count(&model_dir, active_count)?;

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

    /// Compact a single variable-length column.
    ///
    /// Writes a temp file and renames atomically.  Part of the public API for
    /// direct per-column use; `compact_model` uses the staging variant instead.
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

    /// Compact a single fixed-size column.
    ///
    /// Writes a temp file and renames atomically.  Part of the public API for
    /// direct per-column use; `compact_model` uses the staging variant instead.
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

    // -----------------------------------------------------------------------
    // Private staging helpers (write-only, no rename)
    // -----------------------------------------------------------------------

    /// Write compacted variable column data to temp files.
    ///
    /// Returns `[(data_tmp, data_target), (offset_tmp, offset_target)]`.
    ///
    /// # Errors (C3)
    ///
    /// Returns `Err` if any row's `(offset, length)` references bytes outside
    /// the data file.  Silently skipping out-of-range rows would cause data
    /// loss; an error lets the caller decide how to handle corruption.
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
                // C3: inconsistent offset/length is data corruption — return Err
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

        // Write compacted data to temp file
        let temp_data_path = data_path.with_extension("bin.tmp");
        let mut data_file =
            fs::File::create(&temp_data_path).map_err(|e| e.to_string())?;
        data_file.write_all(&new_data).map_err(|e| e.to_string())?;
        data_file.sync_all().map_err(|e| e.to_string())?;
        drop(data_file);

        // Write compacted offsets to temp file
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

    /// Write compacted fixed column data to a temp file.
    ///
    /// Returns `(temp_path, target_path)`.
    ///
    /// `element_size` is derived as `data.len() / tombstones.len()`.  This is
    /// correct only when `tombstones` has been trimmed to the true row count (as
    /// guaranteed by `read_tombstone_bitmap` after the C1 manifest fix).
    fn stage_fixed_column_write(
        column_path: &Path,
        tombstones: &[bool],
    ) -> Result<(PathBuf, PathBuf), String> {
        let mut data = Vec::new();
        fs::File::open(column_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;

        // tombstones.len() == row_count (guaranteed by manifest-based read_tombstone_bitmap)
        // so element_size is always the correct per-column width.
        let element_size = data.len() / tombstones.len();

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

    /// Write a compacted tombstone file to a temp file.
    ///
    /// The storage layer (`forgedb_storage::Tombstones`) uses one byte per row
    /// (`0x00` = alive, non-zero = deleted).  Write exactly `active_count` bytes,
    /// all `0x00` — no deleted rows after compaction.
    ///
    /// Returns `(temp_path, target_path)`.
    fn stage_tombstones_write(
        tombstone_path: &Path,
        tombstones: &[bool],
    ) -> Result<(PathBuf, PathBuf), String> {
        let active_count = tombstones.iter().filter(|&&t| !t).count();
        // One byte per surviving row, all 0x00 (alive).
        let new_bitmap = vec![0u8; active_count];

        let temp_path = tombstone_path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        file.write_all(&new_bitmap).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        Ok((temp_path, tombstone_path.to_path_buf()))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Read the tombstone file and return exactly `row_count` booleans.
    ///
    /// The storage layer (`forgedb_storage::Tombstones`) writes one byte per row:
    /// `0x00` = alive, any non-zero value = deleted.  Reads exactly `row_count`
    /// bytes and maps each to a bool.  Pads with `false` if the file is shorter
    /// than `row_count` (defensively handles partially-written files).
    ///
    /// Uses the authoritative `row_count` from the manifest; the previous
    /// heuristic (try element sizes [16,8,4,1] on the first fixed column file)
    /// was removed because it guesses wrong for models with mixed column widths,
    /// corrupting the element_size calculation for every column.
    fn read_tombstone_bitmap(
        &self,
        path: &Path,
        row_count: usize,
    ) -> Result<Vec<bool>, String> {
        if !path.exists() {
            // No tombstone file → all rows are alive
            return Ok(vec![false; row_count]);
        }

        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        // One byte per row: 0x00 = alive, non-zero = deleted.
        let mut tombstones: Vec<bool> =
            bytes.iter().take(row_count).map(|&b| b != 0).collect();
        // Pad with false if the file is shorter than expected.
        tombstones.resize(row_count, false);
        Ok(tombstones)
    }

    /// Read an offset file into `(offset, length)` pairs.
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
