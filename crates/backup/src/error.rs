use thiserror::Error;

/// Errors from snapshot backup / restore.
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The on-disk layout is inconsistent or incomplete (missing manifest,
    /// anchor shorter than its columns, zero stride, etc.).
    #[error("layout error: {0}")]
    Layout(String),

    /// The archive itself is malformed or an unsupported version.
    #[error("archive format error: {0}")]
    Format(String),
}
