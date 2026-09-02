use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use forgedb_storage::{ColumnKind, Manifest, RowAnchor};
use serde::{Deserialize, Serialize};

mod error;
pub use error::BackupError;

pub type Result<T> = std::result::Result<T, BackupError>;

const MAGIC: &[u8; 8] = b"FORGEBK1";
const CONTAINER_VERSION: u32 = 1;

fn one() -> u32 {
    1
}
const OFFSET_PAIR_BYTES: u64 = 16;
const REPLICATION_LOG: &str = "_replication.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub len: u64,
    #[serde(default)]
    pub base_len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    pub dir: String,
    #[serde(rename = "format_version", default = "one")]
    pub schema_version: u32,
    #[serde(default = "one")]
    pub engine_version: u32,
    pub compaction_epoch: u64,
    pub row_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRef {
    pub chain_id: String,
    pub sequence: u32,
    pub base_row_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveHeader {
    pub container_version: u32,
    pub models: Vec<ModelMeta>,
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub chain_id: String,
    #[serde(default)]
    pub sequence: u32,
    #[serde(default)]
    pub base: Option<BaseRef>,
    #[serde(default)]
    pub base_offset: u64,
}

impl ArchiveHeader {
    pub fn is_incremental(&self) -> bool {
        self.sequence > 0
    }
}

#[derive(Debug, Clone)]
pub struct BackupSummary {
    pub archive_path: PathBuf,
    pub models: Vec<ModelMeta>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct RestoreSummary {
    pub out_dir: PathBuf,
    pub models: Vec<ModelMeta>,
    pub total_bytes: u64,
}

fn default_anchor() -> RowAnchor {
    RowAnchor {
        relative_path: "tombstones.bin".to_string(),
        bytes_per_row: 1,
    }
}

fn offsets_path_for(data_relative_path: &str) -> String {
    data_relative_path.replacen("_data_", "_offsets_", 1)
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

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

    let mut files: BTreeMap<String, u64> = BTreeMap::new();
    let rel = |p: &str| format!("{dir_name}/{p}");

    files.insert(rel("manifest.json"), file_len(&manifest_path));

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
        engine_version: manifest.engine_version,
        compaction_epoch: manifest.compaction_epoch,
        row_count: n,
    };
    let entries = files
        .into_iter()
        .map(|(path, len)| FileEntry {
            path,
            len,
            base_len: 0,
        })
        .collect();
    Ok((meta, entries))
}

fn read_broker_end_offset(data_dir: &Path) -> Result<u64> {
    let log_path = data_dir.join(REPLICATION_LOG);
    let buf = match fs::read(&log_path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };

    let mut pos = 0usize;
    let mut max_offset = 0u64;
    while pos + 4 <= buf.len() {
        let total_len =
            u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        if total_len < 4 + 8 || pos + 4 + total_len > buf.len() {
            break;
        }
        let payload_start = pos + 4;
        let offset = u64::from_le_bytes(
            buf[payload_start..payload_start + 8].try_into().unwrap(),
        );
        if offset > max_offset {
            max_offset = offset;
        }
        pos += 4 + total_len;
    }
    Ok(max_offset)
}

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

pub fn create(data_dir: &Path, archive_path: &Path) -> Result<BackupSummary> {
    if !data_dir.is_dir() {
        return Err(BackupError::Layout(format!(
            "data directory {data_dir:?} does not exist"
        )));
    }

    let base_offset = read_broker_end_offset(data_dir)?;

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
        chain_id: new_chain_id(),
        sequence: 0,
        base: None,
        base_offset,
    };
    let total_bytes = write_archive(archive_path, &header, data_dir)?;

    if let Some((dir, before, after)) = changed_epoch(data_dir, &models) {
        let _ = fs::remove_file(archive_path);
        return Err(BackupError::Layout(format!(
            "{dir}: compaction ran during backup (epoch {before} → {after}); the snapshot would be inconsistent — retry with no concurrent compaction"
        )));
    }

    Ok(BackupSummary {
        archive_path: archive_path.to_path_buf(),
        models,
        total_bytes,
    })
}

pub fn create_incremental(
    data_dir: &Path,
    base_archive: &Path,
    archive_path: &Path,
) -> Result<BackupSummary> {
    if !data_dir.is_dir() {
        return Err(BackupError::Layout(format!(
            "data directory {data_dir:?} does not exist"
        )));
    }
    let base = read_header(base_archive)?;

    let base_offset = read_broker_end_offset(data_dir)?;

    let base_epoch: BTreeMap<&str, u64> = base
        .models
        .iter()
        .map(|m| (m.dir.as_str(), m.compaction_epoch))
        .collect();
    let base_rows: BTreeMap<&str, usize> =
        base.models.iter().map(|m| (m.dir.as_str(), m.row_count)).collect();
    let base_file_len: BTreeMap<&str, u64> =
        base.files.iter().map(|f| (f.path.as_str(), f.len)).collect();

    let mut models = Vec::new();
    let mut files = Vec::new();
    for dir_name in model_dirs(data_dir)? {
        let (meta, entries) = plan_dir(data_dir, &dir_name)?;

        if let Some(&be) = base_epoch.get(dir_name.as_str())
            && be != meta.compaction_epoch
        {
            return Err(BackupError::Layout(format!(
                "{dir_name}: compaction epoch changed since the base ({be} → {}); the append-only byte chain is broken — take a fresh full backup instead of an incremental",
                meta.compaction_epoch
            )));
        }
        if let Some(&br) = base_rows.get(dir_name.as_str())
            && meta.row_count < br
        {
            return Err(BackupError::Layout(format!(
                "{dir_name}: committed rows shrank since the base ({br} → {}) with no epoch bump — not an append-only delta; take a fresh full backup",
                meta.row_count
            )));
        }

        for e in entries {
            let base_len = base_file_len.get(e.path.as_str()).copied().unwrap_or(0);
            if base_len > e.len {
                return Err(BackupError::Layout(format!(
                    "{}: file shrank since the base ({} → {} bytes) — not an append-only delta",
                    e.path, base_len, e.len
                )));
            }
            files.push(FileEntry {
                path: e.path,
                len: e.len,
                base_len,
            });
        }
        models.push(meta);
    }

    let base_row_counts: BTreeMap<String, usize> =
        base.models.iter().map(|m| (m.dir.clone(), m.row_count)).collect();
    let header = ArchiveHeader {
        container_version: CONTAINER_VERSION,
        models: models.clone(),
        files: files.clone(),
        chain_id: base.chain_id.clone(),
        sequence: base.sequence + 1,
        base: Some(BaseRef {
            chain_id: base.chain_id.clone(),
            sequence: base.sequence,
            base_row_counts,
        }),
        base_offset,
    };
    let total_bytes = write_archive(archive_path, &header, data_dir)?;

    if let Some((dir, before, after)) = changed_epoch(data_dir, &models) {
        let _ = fs::remove_file(archive_path);
        return Err(BackupError::Layout(format!(
            "{dir}: compaction ran during backup (epoch {before} → {after}); retry with no concurrent compaction"
        )));
    }

    Ok(BackupSummary {
        archive_path: archive_path.to_path_buf(),
        models,
        total_bytes,
    })
}

fn new_chain_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:032x}-{:x}", nanos, std::process::id())
}

fn write_archive(archive_path: &Path, header: &ArchiveHeader, data_dir: &Path) -> Result<u64> {
    let header_bytes = serde_json::to_vec(header)?;

    if let Some(parent) = archive_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = with_suffix(archive_path, ".tmp");
    let mut total_bytes = 0u64;
    {
        let mut out = BufWriter::new(File::create(&tmp_path)?);
        out.write_all(MAGIC)?;
        out.write_all(&(header_bytes.len() as u32).to_le_bytes())?;
        out.write_all(&header_bytes)?;

        for entry in &header.files {
            let src = data_dir.join(&entry.path);
            let want = entry.len - entry.base_len;
            let written = copy_range(&src, entry.base_len, entry.len, &mut out)?;
            if written != want {
                return Err(BackupError::Layout(format!(
                    "{}: expected {} committed tail bytes but only {} are present (file truncated under snapshot?)",
                    entry.path, want, written
                )));
            }
            total_bytes += written;
        }
        out.flush()?;
        out.get_ref().sync_all()?;
    }
    fs::rename(&tmp_path, archive_path)?;
    Ok(total_bytes)
}

pub fn restore(archive_path: &Path, out_dir: &Path, overwrite: bool) -> Result<RestoreSummary> {
    restore_chain(std::slice::from_ref(&archive_path.to_path_buf()), out_dir, overwrite)
}

fn open_archive(archive_path: &Path) -> Result<(ArchiveHeader, BufReader<File>)> {
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
    Ok((header, reader))
}

pub fn restore_chain(
    archives: &[PathBuf],
    out_dir: &Path,
    overwrite: bool,
) -> Result<RestoreSummary> {
    if archives.is_empty() {
        return Err(BackupError::Format("no archives given to restore".to_string()));
    }

    let headers: Vec<ArchiveHeader> = archives
        .iter()
        .map(|p| read_header(p))
        .collect::<Result<_>>()?;

    if headers[0].sequence != 0 {
        return Err(BackupError::Format(format!(
            "the first archive in a chain must be a full base (sequence 0), got sequence {}",
            headers[0].sequence
        )));
    }
    let chain_id = headers[0].chain_id.clone();
    for (i, h) in headers.iter().enumerate() {
        if h.sequence as usize != i {
            return Err(BackupError::Format(format!(
                "chain is not contiguous: archive #{i} has sequence {} (expected {i}); pass base + deltas in order",
                h.sequence
            )));
        }
        if i > 0 && !chain_id.is_empty() && h.chain_id != chain_id {
            return Err(BackupError::Format(format!(
                "archive #{i} belongs to a different chain ({} != {chain_id})",
                h.chain_id
            )));
        }
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
    for archive_path in archives {
        let (header, mut reader) = open_archive(archive_path)?;
        for entry in &header.files {
            let dest = tmp_dir.join(&entry.path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let current_len = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            if entry.base_len != current_len {
                return Err(BackupError::Format(format!(
                    "chain mismatch for {}: delta assumes {} base bytes but the restored file is {} bytes — archives out of order or from a different base",
                    entry.path, entry.base_len, current_len
                )));
            }
            let want = entry.len - entry.base_len;
            let mut out = BufWriter::new(
                fs::OpenOptions::new()
                    .create(true)
                    .append(entry.base_len > 0)
                    .truncate(entry.base_len == 0)
                    .write(true)
                    .open(&dest)?,
            );
            let written = io::copy(&mut (&mut reader).take(want), &mut out)?;
            out.flush()?;
            if written != want {
                return Err(BackupError::Format(format!(
                    "archive truncated: {} expected {} payload bytes, got {}",
                    entry.path, want, written
                )));
            }
            total_bytes += written;
        }
    }

    let final_models = headers.last().unwrap().models.clone();

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
        models: final_models,
        total_bytes,
    })
}

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

fn copy_range(src: &Path, start: u64, end: u64, out: &mut impl Write) -> Result<u64> {
    if end <= start {
        return Ok(0);
    }
    let mut f = File::open(src)?;
    if start > 0 {
        f.seek(SeekFrom::Start(start))?;
    }
    let mut limited = f.take(end - start);
    Ok(io::copy(&mut limited, out)?)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

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
            engine_version: 1,
            row_count: 0,
            columns: vec![],
            wal_enabled: false,
            last_checkpoint: 0,
            compaction_epoch: epoch,
            row_anchor: Some(RowAnchor {
                relative_path: "tombstones.bin".into(),
                bytes_per_row: 1,
            }),
            auto_sequences: Default::default(),
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
            engine_version: 1,
            compaction_epoch: 4,
            row_count: 0,
        }];

        assert!(changed_epoch(data, &models).is_none());

        write_manifest(&data.join("user"), 5);
        let hit = changed_epoch(data, &models).expect("epoch change must be detected");
        assert_eq!(hit, ("user".to_string(), 4, 5));
    }
}
