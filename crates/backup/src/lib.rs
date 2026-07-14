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
/// The durable replication broker log (#82), a root-level file (not a model dir,
/// so `model_dirs` skips it). Its end offset is the PITR `base_offset` (#77).
const REPLICATION_LOG: &str = "_replication.log";

/// One file's committed prefix, recorded in the archive header. `path` is
/// relative to the data directory root (e.g. `post/tombstones.bin`).
///
/// For a **full** archive `base_len == 0` and the payload carries the whole
/// `[0, len)` prefix. For an **incremental** archive the payload carries only
/// the appended tail `[base_len, len)`; restore concatenates that tail onto the
/// `base_len` bytes the chain's prior members already materialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    /// Committed byte length of this file at snapshot time (the final length
    /// after this archive is applied).
    pub len: u64,
    /// Bytes assumed already present from earlier chain members. `0` for a full
    /// archive; the base's committed length for an incremental. The payload
    /// holds exactly `len - base_len` bytes.
    #[serde(default)]
    pub base_len: u64,
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

/// Records the base an incremental was cut against, so a restore can validate
/// the chain before materializing incompatible bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRef {
    /// The full archive that anchors this chain (0-based `sequence == 0`).
    pub chain_id: String,
    /// This archive's position in the chain (`0` = base, `1..` = deltas).
    pub sequence: u32,
    /// Per-dir base row_count this delta was computed against — restore checks
    /// the running dir length matches before appending the tail.
    pub base_row_counts: BTreeMap<String, usize>,
}

/// The archive header: everything a restore needs to rebuild the tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveHeader {
    pub container_version: u32,
    pub models: Vec<ModelMeta>,
    /// Files in payload order. Restore reads each file's payload bytes in turn
    /// (`len` for a full archive, `len - base_len` for an incremental).
    pub files: Vec<FileEntry>,
    /// A stable identifier for the chain this archive belongs to. A full base
    /// mints a fresh id; each incremental copies its base's id.
    #[serde(default)]
    pub chain_id: String,
    /// `0` for a full base, `1..` for successive incrementals in the chain.
    #[serde(default)]
    pub sequence: u32,
    /// Present iff this is an incremental (`sequence > 0`): the base it deltas.
    #[serde(default)]
    pub base: Option<BaseRef>,
    /// Point-in-time-recovery watermark (#77): the durable-replication-broker
    /// (`_replication.log`, #82) end offset at snapshot time — the highest offset
    /// whose change is already durably present in this archive's committed column
    /// bytes (the generated mutation records to the broker only *after* the column
    /// commit, so any frame `<= base_offset` is a subset of these bytes).
    ///
    /// A follower recovers to a later point by restoring this base then replaying
    /// broker frames `(base_offset .. target]` through the generated `apply_frame`
    /// (#110). `0` when there is no broker log at snapshot time (the `new()`
    /// standalone path, or a data dir that never ran under `open_at`). Layout-blind:
    /// read from the log's frame headers, never from a decoded row.
    #[serde(default)]
    pub base_offset: u64,
}

impl ArchiveHeader {
    /// Whether this archive is an incremental delta (vs a full base).
    pub fn is_incremental(&self) -> bool {
        self.sequence > 0
    }
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
        .map(|(path, len)| FileEntry {
            path,
            len,
            base_len: 0,
        })
        .collect();
    Ok((meta, entries))
}

/// Read the highest offset present in the durable replication log (#82) at
/// `data_dir/_replication.log`, torn-tail-safe. Returns `0` when the log is
/// absent or empty. **Layout-blind**, not schema-decoding: it walks the log's
/// self-describing frame framing (`[4: total_len LE][payload][4: crc]`, the
/// `forgedb-changefeed::durable` on-disk format) and reads only the length
/// prefix + the leading 8-byte offset of each frame — never a model, a kind, or
/// a row byte. This is the PITR `base_offset` watermark (#77).
///
/// It deliberately mirrors the broker's frame walk WITHOUT depending on
/// `forgedb-changefeed` (backup stays a class-1 storage-layout substrate). If
/// the frame framing ever changes, both must move together — the offset is the
/// leading 8 bytes of every payload by construction (see `PersistedEvent::to_frame`).
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
        // A frame is `[4 len][payload (>= 8 for the offset) + 4 crc]`.
        if total_len < 4 + 8 || pos + 4 + total_len > buf.len() {
            break; // torn / incomplete tail — stop at the valid prefix
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

    // Capture the PITR watermark (#77) BEFORE planning columns, so it is a lower
    // bound on the committed state: any broker frame at or below `base_offset`
    // records a mutation whose columns were committed before the broker record
    // (the generated mutation order), hence already in the bytes we copy. A
    // concurrent write past this point only *adds* to the columns — never
    // invalidates a frame `<= base_offset` — so the base stays a superset of
    // everything through `base_offset`.
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

    // Loud fail if compaction ran during the copy (see `changed_epoch`).
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

/// Create an **incremental** archive of `data_dir` against `base_archive`.
///
/// The delta ships only each file's byte tail appended since the base's
/// per-column committed length — cheap because within one compaction epoch the
/// columns are strictly append-only, so bytes below the base watermark are
/// immutable.
///
/// **Epoch guard (chain validity):** a compaction renumbers rows and bumps each
/// dir's `compaction_epoch`, invalidating a byte-tail delta computed against a
/// pre-compaction base. So this refuses (with a clear message) if any current
/// dir's epoch differs from the base's epoch for that dir — the caller must take
/// a fresh full base instead. It also refuses if the current committed length is
/// *shorter* than the base (rows vanished ⇒ not a clean append).
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

    // This incremental's own PITR watermark (#77): captured before planning, same
    // superset argument as `create`. It supersedes the full base's `base_offset`
    // once this delta is applied — recovery replays frames `(base_offset .. target]`.
    let base_offset = read_broker_end_offset(data_dir)?;

    // Index the base's per-dir metadata + per-file committed length.
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

        // Epoch guard: the chain is valid only within one compaction epoch.
        if let Some(&be) = base_epoch.get(dir_name.as_str())
            && be != meta.compaction_epoch
        {
            return Err(BackupError::Layout(format!(
                "{dir_name}: compaction epoch changed since the base ({be} → {}); the append-only byte chain is broken — take a fresh full backup instead of an incremental",
                meta.compaction_epoch
            )));
        }
        // A dir present now but absent from the base is a brand-new dir: its
        // whole prefix ships (base_len 0). A dir shrinking is not a clean append.
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

/// Mint a chain identifier from the wall clock + process id. Opaque; only used
/// to group archives of one chain, never parsed.
fn new_chain_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:032x}-{:x}", nanos, std::process::id())
}

/// Write an archive (full or incremental) to `archive_path` via temp + rename.
/// The payload holds each file's `[base_len, len)` tail (`base_len == 0` ⇒ the
/// whole prefix). Returns the total payload bytes written.
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

/// Restore a single full archive into `out_dir`. Materializes into a temp
/// sibling dir then atomically renames into place — a partial restore never
/// clobbers `out_dir`. Refuses an incremental (needs the chain — use
/// [`restore_chain`]) and an existing non-empty `out_dir` unless `overwrite`.
pub fn restore(archive_path: &Path, out_dir: &Path, overwrite: bool) -> Result<RestoreSummary> {
    restore_chain(std::slice::from_ref(&archive_path.to_path_buf()), out_dir, overwrite)
}

/// Open an archive, validate its magic + container version, and return the
/// header plus a reader positioned at the start of the payload.
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

/// Restore an **ordered chain** (a full base followed by 0+ incrementals in
/// `sequence` order) into `out_dir`. The base materializes each file's full
/// prefix; each incremental *appends* its payload tail onto the running file,
/// validating that the file's current length equals the delta's `base_len`
/// (else the chain is out of order / mismatched). Atomic temp + rename.
///
/// Validates: exactly one base (`sequence == 0`) first, then contiguous
/// `sequence` values sharing one `chain_id`.
pub fn restore_chain(
    archives: &[PathBuf],
    out_dir: &Path,
    overwrite: bool,
) -> Result<RestoreSummary> {
    if archives.is_empty() {
        return Err(BackupError::Format("no archives given to restore".to_string()));
    }

    // Read all headers up front to validate the chain before touching disk.
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
            // Validate the running file length matches the delta's assumed base.
            let current_len = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            if entry.base_len != current_len {
                return Err(BackupError::Format(format!(
                    "chain mismatch for {}: delta assumes {} base bytes but the restored file is {} bytes — archives out of order or from a different base",
                    entry.path, entry.base_len, current_len
                )));
            }
            let want = entry.len - entry.base_len;
            // base_len == 0 truncates+creates; base_len > 0 appends the tail.
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

    // The last header's models describe the final restored state.
    let final_models = headers.last().unwrap().models.clone();

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
        models: final_models,
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

/// Copy the byte range `[start, end)` of `src` into `out`; returns bytes copied
/// (less than `end - start` only if the source is shorter than expected). A
/// full archive passes `start == 0`; an incremental passes `start == base_len`
/// to ship only the appended tail.
fn copy_range(src: &Path, start: u64, end: u64, out: &mut impl Write) -> Result<u64> {
    if end <= start {
        // Nothing appended for this file in this archive (or a 0-row column with
        // no file on disk yet).
        return Ok(0);
    }
    let mut f = File::open(src)?;
    if start > 0 {
        f.seek(SeekFrom::Start(start))?;
    }
    let mut limited = f.take(end - start);
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
