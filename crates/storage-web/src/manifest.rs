use std::io;
use std::path::Path;

use crate::store;

fn default_schema_version() -> u32 {
    1
}

fn default_engine_version() -> u32 {
    1
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    #[serde(rename = "format_version", default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_engine_version")]
    pub engine_version: u32,
    pub row_count: usize,
    pub columns: Vec<ColumnMetadata>,
    #[serde(default)]
    pub wal_enabled: bool,
    #[serde(default)]
    pub last_checkpoint: u64,
    #[serde(default)]
    pub compaction_epoch: u64,
    #[serde(default)]
    pub row_anchor: Option<RowAnchor>,
    #[serde(default)]
    pub auto_sequences: std::collections::BTreeMap<String, u64>,
}

impl Manifest {
    pub fn load_from(path: &Path) -> io::Result<Manifest> {
        store::with_bytes(path, |b| {
            serde_json::from_slice(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
    }

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
    FixedBytes(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowAnchor {
    pub relative_path: String,
    pub bytes_per_row: usize,
}
