//! # forgedb-backup
//!
//! Lock-free **snapshot backup & restore** for a ForgeDB database *directory*,
//! treated as structured-but-opaque storage. A class-1 **substrate** crate: it
//! understands the on-disk *layout* (per-model `manifest.json`, fixed/variable
//! columns, tombstones, junction dirs) and **never** the `.forge` *schema*. It
//! moves opaque file bytes; it does not know any model's fields, relations, or
//! directives. See `docs/proposals/backup-restore.md`.
//!
//! ## Why it is lock-free
//!
//! ForgeDB's storage is **append-only under a row-count anchor**. Each
//! model/junction directory's [`Manifest`] names a `row_anchor` — the file
//! whose length counts committed rows (`tombstones.bin` at 1 byte/row for a
//! model; `fixed/right.bin` at 16 bytes/row for an M2M junction), appended
//! *last* per write. Reading that length gives a committed watermark `N`;
//! every column's committed byte length is a pure function of `N` and the
//! column layout. Concurrent writers only append *past* those lengths, so the
//! copied prefixes never change under the reader — a crash-consistent snapshot
//! with **no lock, no MVCC, no WAL**. A half-finished multi-column write (a
//! torn row: columns appended but the anchor not yet advanced) is naturally
//! excluded, because `N` comes from the last-appended anchor file.
//!
//! ## Archive format (`FORGEBK1`)
//!
//! ```text
//! magic:      8 bytes  b"FORGEBK1"
//! header_len: u32 LE
//! header:     header_len bytes of JSON ([`ArchiveHeader`])
//! payload:    each file's committed prefix, concatenated in header.files order
//! ```
//!
//! The header is versioned (`container_version`) and records each model dir's
//! `schema_version` / `format_version` / `compaction_epoch` / `row_count`, so a
//! restore can refuse incompatible bytes instead of silently materializing them.
//!
//! ## Scope (first milestone, #57)
//!
//! Full snapshot create/restore over a local path, no WAL replay, no
//! incrementals, no cloud targets, no compression. `compaction_epoch` is
//! captured so later incrementals can reject a base taken in a different epoch.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use forgedb_storage::{ColumnKind, Manifest, RowAnchor};
use serde::{Deserialize, Serialize};

mod error;
pub use error::BackupError;

pub type Result<T> = std::result::Result<T, BackupError>;

/// Magic bytes at the head of every archive.
const MAGIC: &[u8; 8] = b"FORGEBK1";
/// Archive container format version (distinct from a model's `format_version`).
const CONTAINER_VERSION: u32 = 1;
/// Bytes per (offset, length) pair in a variable column's offsets file.
const OFFSET_PAIR_BYTES: u64 = 16;

/// One file's committed prefix, recorded in the archive header. `path` is
/// relative to the data directory root (e.g. `post/tombstones.bin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub len: u64,
}

/// Per-directory snapshot metadata (a model or an M2M junction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    /// Directory name relative to the data dir (e.g. `post`, `post_tag_link`).
    pub dir: String,
    pub schema_version: u32,
    pub format_version: u32,
    pub compaction_epoch: u64,
    /// Committed row count at snapshot time (`len(anchor) / bytes_per_row`).
    pub row_count: usize,
}

/// The archive header: everything a restore needs to rebuild the tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveHeader {
    pub container_version: u32,
    pub models: Vec<ModelMeta>,
    /// Files in payload order. Restore reads each file's `len` bytes in turn.
    pub files: Vec<FileEntry>,
}

/// Result of a successful [`create`].
#[derive(Debug, Clone)]
pub struct BackupSummary {
    pub archive_path: PathBuf,
    pub models: Vec<ModelMeta>,
    /// Total committed payload bytes (excludes the header).
    pub total_bytes: u64,
}

/// Result of a successful [`restore`].
#[derive(Debug, Clone)]
pub struct RestoreSummary {
    pub out_dir: PathBuf,
    pub models: Vec<ModelMeta>,
    pub total_bytes: u64,
}

/// Fall back to this anchor for a legacy manifest that predates `row_anchor`.
fn default_anchor() -> RowAnchor {
    RowAnchor {
        relative_path: "tombstones.bin".to_string(),
        bytes_per_row: 1,
    }
}

/// Derive a variable column's offsets file path from its data file path, per
/// the storage-layout convention (`variable/string_data_N.bin` →
/// `variable/string_offsets_N.bin`). Substrate layout knowledge, not schema.
fn offsets_path_for(data_relative_path: &str) -> String {
    data_relative_path.replacen("_data_", "_offsets_", 1)
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Committed length of a variable column's data file for `n` rows: the end of
/// the `n`-th record = `offset[n-1] + length[n-1]`, read from the offsets file.
/// Returns 0 for `n == 0`.
fn variable_data_committed_len(offsets_path: &Path, n: usize) -> Result<u64> {
    if n == 0 {
        return Ok(0);
    }
    let last = (n as u64 - 1) * OFFSET_PAIR_BYTES;
    let mut f = File::open(offsets_path)?;
    f.seek(SeekFrom::Start(last))?;
    let mut buf = [0u8; OFFSET_PAIR_BYTES as usize];
    f.read_exact(&mut buf).map_err(|e| {
        BackupError::Layout(format!(
            "offsets file {:?} is shorter than the {} rows the anchor reports: {}",
            offsets_path, n, e
        ))
    })?;
    let offset = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let length = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    Ok(offset + length)
}

/// Plan the committed file set for one model/junction directory. Reads only the
/// directory's `manifest.json` (layout) and stats — never the schema.
fn plan_dir(data_dir: &Path, dir_name: &str) -> Result<(ModelMeta, Vec<FileEntry>)> {
    let dir = data_dir.join(dir_name);
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(BackupError::Layout(format!(
            "{dir_name}/manifest.json is missing; cannot snapshot a directory without a layout manifest"
        )));
    }
    let manifest: Manifest = Manifest::load_from(&manifest_path)?;

    let anchor = manifest.row_anchor.clone().unwrap_or_else(default_anchor);
    if anchor.bytes_per_row == 0 {
        return Err(BackupError::Layout(format!(
            "{dir_name}: row_anchor.bytes_per_row is 0"
        )));
    }
    let anchor_abs = dir.join(&anchor.relative_path);
    let n = (file_len(&anchor_abs) / anchor.bytes_per_row as u64) as usize;

    // Dedupe by path: a junction's `right.bin` is both a column and the anchor.
    let mut files: BTreeMap<String, u64> = BTreeMap::new();
    let rel = |p: &str| format!("{dir_name}/{p}");

    // The manifest itself travels verbatim so the restored dir is self-describing.
    files.insert(rel("manifest.json"), file_len(&manifest_path));

    // The anchor file's committed prefix.
    files.insert(rel(&anchor.relative_path), n as u64 * anchor.bytes_per_row as u64);

    for col in &manifest.columns {
        match col.kind {
            ColumnKind::Fixed => {
                files.insert(rel(&col.relative_path), n as u64 * col.value_size as u64);
            }
            ColumnKind::Variable => {
                let offsets_rel = offsets_path_for(&col.relative_path);
                let offsets_abs = dir.join(&offsets_rel);
                let data_len = variable_data_committed_len(&offsets_abs, n)?;
                files.insert(rel(&col.relative_path), data_len);
                files.insert(rel(&offsets_rel), n as u64 * OFFSET_PAIR_BYTES);
            }
        }
    }

    let meta = ModelMeta {
        dir: dir_name.to_string(),
        schema_version: manifest.schema_version,
        format_version: manifest.format_version,
        compaction_epoch: manifest.compaction_epoch,
        row_count: n,
    };
    let entries = files
        .into_iter()
        .map(|(path, len)| FileEntry { path, len })
        .collect();
    Ok((meta, entries))
}

/// Enumerate the model/junction directories in a data dir (immediate
/// subdirectories, skipping dotfiles), sorted for deterministic archives.
fn model_dirs(data_dir: &Path) -> Result<Vec<String>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && !name.starts_with('.')
        {
            dirs.push(name.to_string());
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Create a lock-free full-snapshot archive of `data_dir` at `archive_path`.
///
/// Safe to run while a generated app is appending: each file is copied only up
/// to its committed length (derived from the row-count anchor), so concurrent
/// appends past that point are excluded and no torn row is captured.
pub fn create(data_dir: &Path, archive_path: &Path) -> Result<BackupSummary> {
    if !data_dir.is_dir() {
        return Err(BackupError::Layout(format!(
            "data directory {data_dir:?} does not exist"
        )));
    }

    let mut models = Vec::new();
    let mut files = Vec::new();
    for dir_name in model_dirs(data_dir)? {
        let (meta, entries) = plan_dir(data_dir, &dir_name)?;
        models.push(meta);
        files.extend(entries);
    }

    let header = ArchiveHeader {
        container_version: CONTAINER_VERSION,
        models: models.clone(),
        files: files.clone(),
    };
    let header_bytes = serde_json::to_vec(&header)?;

    if let Some(parent) = archive_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    // Write to a temp file then atomically rename, so a crash never leaves a
    // half-written archive that looks valid.
    let tmp_path = with_suffix(archive_path, ".tmp");
    let mut total_bytes = 0u64;
    {
        let mut out = BufWriter::new(File::create(&tmp_path)?);
        out.write_all(MAGIC)?;
        out.write_all(&(header_bytes.len() as u32).to_le_bytes())?;
        out.write_all(&header_bytes)?;

        for entry in &files {
            let src = data_dir.join(&entry.path);
            let written = copy_prefix(&src, entry.len, &mut out)?;
            if written != entry.len {
                return Err(BackupError::Layout(format!(
                    "{}: expected {} committed bytes but only {} are present (file truncated under snapshot?)",
                    entry.path, entry.len, written
                )));
            }
            total_bytes += written;
        }
        out.flush()?;
        out.get_ref().sync_all()?;
    }

    // Loud fail if compaction ran during the copy. Compaction rewrites fixed/
    // variable files (shifting every offset) and bumps `compaction_epoch`, so a
    // copy that straddled it mixes pre- and post-rewrite bytes and is silently
    // corrupt. Re-read each dir's epoch; a change means our prefixes are stale.
    // (Backup and compaction are required not to overlap; this catches the
    // accident rather than trusting the operator.)
    if let Some((dir, before, after)) = changed_epoch(data_dir, &models) {
        let _ = fs::remove_file(&tmp_path);
        return Err(BackupError::Layout(format!(
            "{dir}: compaction ran during backup (epoch {before} → {after}); the snapshot would be inconsistent — retry with no concurrent compaction"
        )));
    }

    fs::rename(&tmp_path, archive_path)?;

    Ok(BackupSummary {
        archive_path: archive_path.to_path_buf(),
        models,
        total_bytes,
    })
}

/// Restore an archive into `out_dir`. Materializes into a temp sibling dir then
/// atomically renames into place — a partial restore never clobbers `out_dir`.
///
/// Refuses to overwrite an existing non-empty `out_dir` unless `overwrite`.
pub fn restore(archive_path: &Path, out_dir: &Path, overwrite: bool) -> Result<RestoreSummary> {
    let mut reader = BufReader::new(File::open(archive_path)?);

    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(BackupError::Format(
            "not a ForgeDB backup archive (bad magic)".to_string(),
        ));
    }
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let header_len = u32::from_le_bytes(len_buf) as usize;
    let mut header_bytes = vec![0u8; header_len];
    reader.read_exact(&mut header_bytes)?;
    let header: ArchiveHeader = serde_json::from_slice(&header_bytes)?;

    if header.container_version != CONTAINER_VERSION {
        return Err(BackupError::Format(format!(
            "unsupported archive container version {} (this build reads {})",
            header.container_version, CONTAINER_VERSION
        )));
    }

    let exists_nonempty = out_dir.is_dir()
        && fs::read_dir(out_dir)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false);
    if exists_nonempty && !overwrite {
        return Err(BackupError::Layout(format!(
            "refusing to restore over existing non-empty directory {out_dir:?} (pass overwrite to replace)"
        )));
    }

    let tmp_dir = with_suffix(out_dir, ".restore-tmp");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)?;
    }
    fs::create_dir_all(&tmp_dir)?;

    let mut total_bytes = 0u64;
    for entry in &header.files {
        let dest = tmp_dir.join(&entry.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = BufWriter::new(File::create(&dest)?);
        let written = io::copy(&mut (&mut reader).take(entry.len), &mut out)?;
        out.flush()?;
        if written != entry.len {
            return Err(BackupError::Format(format!(
                "archive truncated: {} expected {} bytes, got {}",
                entry.path, entry.len, written
            )));
        }
        total_bytes += written;
    }

    // Swap into place atomically. Remove the prior dir only after the temp tree
    // is fully materialized.
    if out_dir.exists() {
        fs::remove_dir_all(out_dir)?;
    } else if let Some(parent) = out_dir.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&tmp_dir, out_dir)?;

    Ok(RestoreSummary {
        out_dir: out_dir.to_path_buf(),
        models: header.models,
        total_bytes,
    })
}

/// Read the header of an archive without materializing it — for `verify`/`list`.
pub fn read_header(archive_path: &Path) -> Result<ArchiveHeader> {
    let mut reader = BufReader::new(File::open(archive_path)?);
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(BackupError::Format(
            "not a ForgeDB backup archive (bad magic)".to_string(),
        ));
    }
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let header_len = u32::from_le_bytes(len_buf) as usize;
    let mut header_bytes = vec![0u8; header_len];
    reader.read_exact(&mut header_bytes)?;
    Ok(serde_json::from_slice(&header_bytes)?)
}

/// Copy exactly the first `len` bytes of `src` into `out`; returns bytes copied
/// (which is less than `len` only if the source is shorter than expected).
fn copy_prefix(src: &Path, len: u64, out: &mut impl Write) -> Result<u64> {
    if len == 0 {
        // A zero-length column (0 rows) may not have a file on disk yet.
        return Ok(0);
    }
    let f = File::open(src)?;
    let mut limited = f.take(len);
    Ok(io::copy(&mut limited, out)?)
}

/// Append a suffix to a path's final component (`foo` + `.tmp` → `foo.tmp`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Re-read each dir's current `compaction_epoch` and return the first one that
/// differs from the epoch captured at plan time — the signal that compaction
/// rewrote files under the copy. `(dir, before, after)`; `None` if all stable.
fn changed_epoch(data_dir: &Path, models: &[ModelMeta]) -> Option<(String, u64, u64)> {
    for meta in models {
        let manifest_path = data_dir.join(&meta.dir).join("manifest.json");
        let after = Manifest::load_from(&manifest_path)
            .map(|m| m.compaction_epoch)
            .unwrap_or(meta.compaction_epoch);
        if after != meta.compaction_epoch {
            return Some((meta.dir.clone(), meta.compaction_epoch, after));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_storage::{Manifest, RowAnchor};

    fn write_manifest(dir: &Path, epoch: u64) {
        fs::create_dir_all(dir).unwrap();
        let manifest = Manifest {
            schema_version: 1,
            row_count: 0,
            columns: vec![],
            wal_enabled: false,
            last_checkpoint: 0,
            compaction_epoch: epoch,
            format_version: 1,
            row_anchor: Some(RowAnchor {
                relative_path: "tombstones.bin".into(),
                bytes_per_row: 1,
            }),
        };
        manifest.save_to(&dir.join("manifest.json")).unwrap();
    }

    #[test]
    fn changed_epoch_detects_concurrent_compaction() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        write_manifest(&data.join("user"), 4);

        let models = vec![ModelMeta {
            dir: "user".into(),
            schema_version: 1,
            format_version: 1,
            compaction_epoch: 4,
            row_count: 0,
        }];

        // Stable epoch → no change detected.
        assert!(changed_epoch(data, &models).is_none());

        // Simulate compaction bumping the epoch after plan time.
        write_manifest(&data.join("user"), 5);
        let hit = changed_epoch(data, &models).expect("epoch change must be detected");
        assert_eq!(hit, ("user".to_string(), 4, 5));
    }
}
