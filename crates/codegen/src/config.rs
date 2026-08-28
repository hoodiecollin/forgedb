use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncMode {
    Always,
    Never,
}

impl FsyncMode {
    pub fn wal_policy_variant(self) -> &'static str {
        match self {
            FsyncMode::Always => "Always",
            FsyncMode::Never => "Never",
        }
    }
}

impl fmt::Display for FsyncMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FsyncMode::Always => "always",
            FsyncMode::Never => "never",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenConfig {
    pub replication: bool,

    pub fsync: FsyncMode,

    pub wal_checkpoint_interval: u64,

    pub compaction: bool,

    pub compaction_threshold: u64,

    pub changefeed_capacity: usize,

    pub max_cascade_depth: u32,

    pub txn_max_retries: u32,

    pub page_default_limit: usize,

    pub page_max_limit: usize,

    pub metrics: bool,

    pub wasm_commit_debounce_ms: u64,

    pub wasm_commit_max_frames: u64,

    pub replication_log_retention: u64,

    pub web: bool,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl GenConfig {
    pub const DEFAULT: Self = Self {
        replication: false,
        fsync: FsyncMode::Always,
        wal_checkpoint_interval: 1000,
        compaction: true,
        compaction_threshold: 1000,
        changefeed_capacity: 1024,
        max_cascade_depth: 64,
        txn_max_retries: 3,
        page_default_limit: 50,
        page_max_limit: 1000,
        web: true,
        metrics: true,
        wasm_commit_debounce_ms: 250,
        wasm_commit_max_frames: 100,
        replication_log_retention: 0,
    };

    pub const fn needs_utoipa(&self) -> bool {
        self.web
    }

    pub fn legacy_with_replication() -> Self {
        Self {
            replication: true,
            ..Self::default()
        }
    }
}
