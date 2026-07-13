//! Physical-layout metadata types, mirroring `forgedb-storage-native`.
//!
//! These are pure serde structs (no schema semantics). They are duplicated here
//! rather than shared so the native crate's published surface stays untouched;
//! because the `forgedb-storage` facade links exactly one backend per build, the
//! two identical definitions never coexist. `save_to`/`load_from` route through
//! the arena [`crate::store`] instead of the filesystem so the generated
//! `write_manifest` call compiles and runs unchanged on this target.

use std::io;
use std::path::Path;

use crate::store;

fn default_format_version() -> u32 {
    1
}

/// Physical-layout metadata (see the native crate's `Manifest` for the full doc).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub row_count: usize,
    pub columns: Vec<ColumnMetadata>,
    #[serde(default)]
    pub wal_enabled: bool,
    #[serde(default)]
    pub last_checkpoint: u64,
    #[serde(default)]
    pub compaction_epoch: u64,
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub row_anchor: Option<RowAnchor>,
}

impl Manifest {
    /// Load a manifest from an arena blob (the browser analogue of reading
    /// `<model>/manifest.json`).
    pub fn load_from(path: &Path) -> io::Result<Manifest> {
        store::with_bytes(path, |b| {
            serde_json::from_slice(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
    }

    /// Write this manifest into its arena blob so a later `commit` persists it to
    /// IndexedDB/OPFS. There is no temp-file+rename dance on this target — an
    /// arena write is atomic within the single-threaded follower.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let content = serde_json::to_vec_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        store::with_bytes_mut(path, |b| {
            b.clear();
            b.extend_from_slice(&content);
        });
        Ok(())
    }
}

/// Whether a column is fixed-width or variable-length. Physical-layout fact only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnKind {
    #[default]
    Fixed,
    Variable,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub column_type: ColumnType,
    pub column_index: usize,
    #[serde(default)]
    pub value_size: usize,
    #[serde(default)]
    pub kind: ColumnKind,
    #[serde(default)]
    pub relative_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColumnType {
    #[default]
    U32,
    U64,
    I32,
    I64,
    F64,
    Bool,
    Uuid,
    Timestamp,
    String,
    /// Fixed-size byte array (for char(N), structs, fixed arrays).
    FixedBytes(usize),
}

/// Physical descriptor of the file/blob whose length counts committed rows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowAnchor {
    pub relative_path: String,
    pub bytes_per_row: usize,
}
