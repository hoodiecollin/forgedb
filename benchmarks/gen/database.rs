#![allow(dead_code, unused_imports, irrefutable_let_patterns, unused_mut)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};
use forgedb_types::{Uuid, Timestamp, Value};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
const WAL_CHECKPOINT_INTERVAL: u64 = 1000;
const COMPACTION_DEAD_THRESHOLD: u64 = 1000;
const COMPACTION_DEAD_CEILING_FACTOR: u64 = 4;
const EXPECTED_SCHEMA_VERSION: u32 = 1;
const EXPECTED_ENGINE_VERSION: u32 = 2;
fn __forgedb_default_ts() -> Timestamp {
    Timestamp::from_micros(0)
}
fn __forgedb_ws_key(parts: &[&[u8]]) -> Vec<u8> {
    let mut k = Vec::with_capacity(parts.iter().map(|p| p.len() + 4).sum::<usize>());
    for p in parts {
        k.extend_from_slice(&(p.len() as u32).to_le_bytes());
        k.extend_from_slice(p);
    }
    k
}
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    Unique { model: &'static str, field: &'static str },
    DanglingReference { model: &'static str, field: &'static str, target: &'static str },
    ReferencedByChildren { model: &'static str, field: &'static str },
    Constraint {
        model: &'static str,
        field: &'static str,
        rule: &'static str,
        message: String,
    },
    SequenceExhausted { model: &'static str, field: &'static str },
}
impl ValidationError {
    pub fn status_code(&self) -> u16 {
        match self {
            ValidationError::Unique { .. }
            | ValidationError::DanglingReference { .. }
            | ValidationError::ReferencedByChildren { .. } => 409,
            ValidationError::Constraint { .. } => 422,
            ValidationError::SequenceExhausted { .. } => 500,
        }
    }
}
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Unique { model, field } => {
                write!(f, "unique constraint violated on field `{}.{}`", model, field)
            }
            ValidationError::DanglingReference { model, field, target } => {
                write!(
                    f, "field `{}.{}` references a non-existent {}", model, field, target
                )
            }
            ValidationError::ReferencedByChildren { model, field } => {
                write!(
                    f,
                    "cannot delete: {} rows still reference it via `{}` (on_delete=restrict)",
                    model, field
                )
            }
            ValidationError::Constraint { model, field, rule, message } => {
                write!(f, "field `{}.{}` violates `{}`: {}", model, field, rule, message)
            }
            ValidationError::SequenceExhausted { model, field } => {
                write!(
                    f, "auto-increment sequence for `{}.{}` is exhausted", model, field
                )
            }
        }
    }
}
impl std::error::Error for ValidationError {}
#[derive(Debug)]
pub enum ApplyError {
    Decode(String),
    Validation(ValidationError),
}
impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Decode(m) => {
                write!(f, "failed to decode replication frame: {}", m)
            }
            ApplyError::Validation(e) => write!(f, "replicated write rejected: {}", e),
        }
    }
}
impl std::error::Error for ApplyError {}
impl From<ValidationError> for ApplyError {
    fn from(e: ValidationError) -> Self {
        ApplyError::Validation(e)
    }
}
#[derive(Debug)]
pub enum TxError {
    Validation(ValidationError),
    Io(String),
    Conflict,
    CoordinatorUnavailable(String),
}
impl TxError {
    pub fn status_code(&self) -> u16 {
        match self {
            TxError::Validation(e) => e.status_code(),
            TxError::Io(_) => 500,
            TxError::Conflict => 409,
            TxError::CoordinatorUnavailable(_) => 503,
        }
    }
}
impl std::fmt::Display for TxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxError::Validation(e) => write!(f, "transaction rejected: {}", e),
            TxError::Io(m) => write!(f, "transaction I/O failure: {}", m),
            TxError::Conflict => write!(f, "transaction conflict (retry)"),
            TxError::CoordinatorUnavailable(m) => {
                write!(f, "commit coordinator unavailable: {}", m)
            }
        }
    }
}
impl std::error::Error for TxError {}
impl From<ValidationError> for TxError {
    fn from(e: ValidationError) -> Self {
        TxError::Validation(e)
    }
}
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    #[serde(default)]
    pub id: Uuid,
    pub name: String,
    pub email: String,
    #[schema(value_type = String)]
    #[serde(default = "__forgedb_default_ts")]
    pub created_at: Timestamp,
    pub posts: (),
}
#[derive(Debug, Clone)]
pub struct UserScanRef<'a> {
    pub __slot: usize,
    pub id: Uuid,
    pub name: &'a str,
    pub email: &'a str,
    pub created_at: Timestamp,
}
#[derive(serde::Serialize)]
pub struct UserPageRef<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub email: &'a str,
    pub created_at: Timestamp,
    pub posts: (),
}
pub struct UserStorage {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    row_count: usize,
    id_col: FixedColumn,
    name_col: VariableColumn,
    email_col: VariableColumn,
    created_at_col: FixedColumn,
    email_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::Tombstones,
    wal: forgedb_wal::WalManager,
    writes_since_checkpoint: u64,
    root: std::path::PathBuf,
    dead_since_compaction: u64,
    in_transaction: bool,
    checkpoint_deferred: bool,
    compact_deferred: bool,
    compaction_due: bool,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl UserStorage {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("user/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            name_col: VariableColumn::new(
                    root.join("user/variable/string_data_1.bin"),
                    root.join("user/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            email_col: VariableColumn::new(
                    root.join("user/variable/string_data_2.bin"),
                    root.join("user/variable/string_offsets_2.bin"),
                )
                .expect("Failed to create variable column"),
            created_at_col: FixedColumn::new(
                    root.join("user/fixed/timestamp_3.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            email_index: std::sync::Arc::new(std::collections::HashMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("user/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("user/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let n = db.tombstones.len();
        db.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = db.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut db.id_versions).entry(id).or_default().push(i);
        }
        let __ids: Vec<Uuid> = db.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match db.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if db.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let email_value = db
                .email_col
                .read_string(__row)
                .expect("Failed to read string");
            {
                let __k: String = {
                    {
                        let __v = &(email_value);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut db.email_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
        }
        let _ = db.write_manifest(root);
        db
    }
    fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("user/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            name_col: VariableColumn::new(
                    root.join("user/variable/string_data_1.bin"),
                    root.join("user/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            email_col: VariableColumn::new(
                    root.join("user/variable/string_data_2.bin"),
                    root.join("user/variable/string_offsets_2.bin"),
                )
                .expect("Failed to create variable column"),
            created_at_col: FixedColumn::new(
                    root.join("user/fixed/timestamp_3.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            email_index: std::sync::Arc::new(std::collections::HashMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("user/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("user/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let _ = db.write_manifest(root);
        db
    }
    fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "id".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_0.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "name".to_string(), column_type : forgedb_storage::ColumnType::String,
            column_index : 1usize, value_size : 0usize, kind :
            forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_1.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "email".to_string(), column_type :
            forgedb_storage::ColumnType::String, column_index : 2usize, value_size :
            0usize, kind : forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_2.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "created_at".to_string(), column_type :
            forgedb_storage::ColumnType::Timestamp, column_index : 3usize, value_size :
            8usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/timestamp_3.bin".to_string(), }
        ];
        let __manifest_abs = root.join("user/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.auto_sequences)
            .unwrap_or_default();
        #[allow(unused_mut)]
        let mut __auto_sequences = __persisted_seqs;
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "tombstones.bin".to_string(),
                bytes_per_row: 1usize,
            }),
            auto_sequences: __auto_sequences,
        };
        manifest.save_to(&__manifest_abs)
    }
    fn bump_compaction_epoch(&self, root: &std::path::Path) {
        let __manifest_abs = root.join("user/manifest.json");
        if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
            __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
            let _ = __m.save_to(&__manifest_abs);
        }
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    pub fn insert(&mut self, record: User) -> Result<Uuid, ValidationError> {
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.email_index.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                return Err(ValidationError::Unique {
                    model: "User",
                    field: "email",
                });
            }
        }
        let row_index = self.row_count;
        let id = record.id;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("User", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.email_col.append_string(&record.email).expect("Failed to append string");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.email_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("User", row_index, forgedb_changefeed::ChangeKind::Inserted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "User",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Inserted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        Ok(id)
    }
    pub fn update(&mut self, id: Uuid, record: User) -> Result<bool, ValidationError> {
        if !self.id_to_row.contains_key(&id) {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if let Some(__ids) = self.email_index.get(&__uk) {
                if __ids.iter().any(|__i| *__i != id) {
                    return Err(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    });
                }
            }
        }
        let __old = self.get(id);
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("User", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.email_col.append_string(&record.email).expect("Failed to append string");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(__old_rec) = &__old {
            {
                let __k: String = {
                    {
                        let __v = &(__old_rec.email);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.email_index);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
        }
        {
            let __k: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.email_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("User", row_index, forgedb_changefeed::ChangeKind::Updated);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "User",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Updated,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        Ok(true)
    }
    pub fn delete(&mut self, id: Uuid) -> bool {
        let record = match self.get(id) {
            Some(r) => r,
            None => return false,
        };
        let deleted_row = *self
            .id_to_row
            .get(&id)
            .expect("id present: get succeeded above");
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(1u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("User", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.email_col.append_string(&record.email).expect("Failed to append string");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(true).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.email_index);
            if let Some(__set) = __map.get_mut(&__k) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__k);
            }
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("User", deleted_row, forgedb_changefeed::ChangeKind::Deleted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "User",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        true
    }
    pub fn apply(
        &mut self,
        kind: forgedb_changefeed::ChangeKind,
        bytes: &[u8],
    ) -> Result<(), ApplyError> {
        match kind {
            forgedb_changefeed::ChangeKind::Inserted => {
                let record: User = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                self.insert(record)?;
            }
            forgedb_changefeed::ChangeKind::Updated => {
                let record: User = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.update(id, record)?;
            }
            forgedb_changefeed::ChangeKind::Deleted => {
                let record: User = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.delete(id);
            }
            forgedb_changefeed::ChangeKind::Linked => {}
        }
        Ok(())
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.id_col.sync_to_drive()?;
        self.name_col.sync_to_drive()?;
        self.email_col.sync_to_drive()?;
        self.created_at_col.sync_to_drive()?;
        self.tombstones.sync_to_drive()?;
        self.tombstones.barrier()?;
        Ok(())
    }
    fn recover_from_wal(&mut self) {
        let __anchor = self.tombstones.len();
        while self.id_col.len() < __anchor {
            self.id_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.name_col.len() < __anchor {
            self.name_col.append_string("").expect("Failed to backfill string column");
        }
        while self.email_col.len() < __anchor {
            self.email_col.append_string("").expect("Failed to backfill string column");
        }
        while self.created_at_col.len() < __anchor {
            self.created_at_col
                .append_timestamp(0i64)
                .expect("Failed to backfill column");
        }
        self.id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.name_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.email_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.created_at_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.row_count = __anchor;
        let __entries = self
            .wal
            .replay(|_| -> std::io::Result<()> { Ok(()) })
            .expect("Failed to replay WAL on recovery");
        for __entry in &__entries {
            if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                if payload.len() < 9 {
                    continue;
                }
                let mut __ri = [0u8; 8];
                __ri.copy_from_slice(&payload[0..8]);
                let __row_index = u64::from_le_bytes(__ri) as usize;
                if __row_index < self.row_count {
                    continue;
                }
                let __deleted = payload[8] != 0;
                let record: User = match serde_json::from_slice(&payload[9..]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                self.id_col
                    .append_uuid(*record.id.as_bytes())
                    .expect("Failed to append to column");
                self.name_col
                    .append_string(&record.name)
                    .expect("Failed to append string");
                self.email_col
                    .append_string(&record.email)
                    .expect("Failed to append string");
                self.created_at_col
                    .append_timestamp(i64::from(record.created_at))
                    .expect("Failed to append to column");
                self.tombstones
                    .append(__deleted)
                    .expect("Failed to append tombstone on recovery");
                self.row_count += 1;
            }
        }
    }
    pub fn checkpoint(&mut self) {
        self.id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.name_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.email_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.created_at_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync tombstones to drive on checkpoint");
        self.tombstones.barrier().expect("Failed to issue checkpoint device barrier");
        self.wal.truncate().expect("Failed to truncate WAL on checkpoint");
        self.writes_since_checkpoint = 0;
    }
    pub fn compact(&mut self) {
        if self.in_transaction {
            self.compact_deferred = true;
            return;
        }
        self.checkpoint();
        let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
        for &__row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                __keep.push(__row);
            }
        }
        let __config = forgedb_compaction::CompactionConfig::default();
        let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
        if __compactor.compact_model_keeping("user", &__keep).is_ok() {
            let mut __keep_sorted = __keep;
            __keep_sorted.sort_unstable();
            let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
            let __saved_email_index = std::sync::Arc::clone(&self.email_index);
            let __root = self.root.clone();
            let __feed = self.changefeed.take();
            let __broker = self.broker.take();
            *self = Self::new_at_no_rehydrate(&__root);
            self.changefeed = __feed;
            self.broker = __broker;
            self.email_index = __saved_email_index;
            let mut __new_id_to_row: std::collections::HashMap<_, usize> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            for (&__id, &__old_row) in __old_id_to_row.iter() {
                if __keep_sorted.binary_search(&__old_row).is_ok() {
                    let __new_row = __keep_sorted
                        .partition_point(|&__r| __r < __old_row);
                    __new_id_to_row.insert(__id, __new_row);
                    __new_id_versions.insert(__id, vec![__new_row]);
                }
            }
            self.id_to_row = std::sync::Arc::new(__new_id_to_row);
            self.id_versions = std::sync::Arc::new(__new_id_versions);
            self.bump_compaction_epoch(&__root);
        }
        self.dead_since_compaction = 0;
        self.compaction_due = false;
    }
    pub fn maintain(&mut self) {
        if self.compaction_due && !self.in_transaction {
            self.compaction_due = false;
            self.compact();
        }
    }
    pub fn __reindex_committed(&mut self) {
        std::sync::Arc::make_mut(&mut self.id_to_row).clear();
        std::sync::Arc::make_mut(&mut self.id_versions).clear();
        std::sync::Arc::make_mut(&mut self.email_index).clear();
        let n = self.tombstones.len();
        self.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = self.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(i);
        }
        let __ids: Vec<Uuid> = self.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match self.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if self.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let email_value = self
                .email_col
                .read_string(__row)
                .expect("Failed to read string");
            {
                let __k: String = {
                    {
                        let __v = &(email_value);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut self.email_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
        }
    }
    pub fn __reindex_delta(&mut self, from: usize) {
        let __n = self.row_count;
        for __r in from..__n {
            let id = {
                let bytes = self
                    .id_col
                    .read_uuid(__r)
                    .expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            if let Some(__old_rec) = self.get(id) {
                {
                    let __k: String = {
                        {
                            let __v = &(__old_rec.email);
                            let mut __k = String::with_capacity(1 + __v.len());
                            __k.push('\u{1}');
                            __k.push_str(__v);
                            __k
                        }
                    };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.email_index);
                    if let Some(__set) = __map.get_mut(&__k) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__k);
                    }
                }
            }
            let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(__r);
            if !__deleted {
                if let Some(__new_rec) = self.read_at(__r) {
                    {
                        let __k: String = {
                            {
                                let __v = &(__new_rec.email);
                                let mut __k = String::with_capacity(1 + __v.len());
                                __k.push('\u{1}');
                                __k.push_str(__v);
                                __k
                            }
                        };
                        std::sync::Arc::make_mut(&mut self.email_index)
                            .entry(__k)
                            .or_default()
                            .insert(id);
                    }
                }
            }
        }
    }
    pub fn __sync_columns_from_disk(&mut self) {
        let _ = self.id_col.sync_from_disk();
        let _ = self.name_col.sync_from_disk();
        let _ = self.email_col.sync_from_disk();
        let _ = self.created_at_col.sync_from_disk();
        let _ = self.tombstones.sync_from_disk();
        self.row_count = self.tombstones.len();
    }
    pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
        self.id_col.truncate_to_rows(rows)?;
        self.name_col.truncate_to_rows(rows)?;
        self.email_col.truncate_to_rows(rows)?;
        self.created_at_col.truncate_to_rows(rows)?;
        self.tombstones.truncate_to_rows(rows)?;
        Ok(())
    }
    pub fn __recover_to_committed(&mut self, committed_len: usize) {
        if committed_len >= self.row_count {
            return;
        }
        self.id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.name_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.email_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.created_at_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.tombstones
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate tombstones on journal recovery");
        self.row_count = committed_len;
        self.wal.truncate().expect("Failed to truncate WAL on journal recovery");
        let __root = self.root.clone();
        let __feed = self.changefeed.take();
        let __broker = self.broker.take();
        *self = Self::new_at(&__root);
        self.changefeed = __feed;
        self.broker = __broker;
    }
    pub fn __stage_append(&mut self, record: User, deleted: bool) -> usize {
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        if deleted {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(1u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("User", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        } else {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(0u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("User", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.email_col.append_string(&record.email).expect("Failed to append string");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(deleted).expect("Failed to append tombstone");
        self.row_count += 1;
        row_index
    }
    pub fn run_deferred_maintenance(&mut self) {
        if self.compact_deferred {
            self.compact_deferred = false;
            self.checkpoint_deferred = false;
            self.compact();
        } else if self.checkpoint_deferred {
            self.checkpoint_deferred = false;
            self.checkpoint();
        }
    }
    pub fn read_at(&self, row_index: usize) -> Option<User> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let name_value = self
            .name_col
            .read_string(row_index)
            .expect("Failed to read string");
        let email_value = self
            .email_col
            .read_string(row_index)
            .expect("Failed to read string");
        let created_at_value = Timestamp::from(
            self
                .created_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        Some(User {
            id: id_value,
            name: name_value,
            email: email_value,
            created_at: created_at_value,
            posts: (),
        })
    }
    pub fn get(&self, id: Uuid) -> Option<User> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_at(row_index)
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<User> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<User> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn all(&self) -> Vec<User> {
        let mut records = Vec::new();
        for id in self.id_to_row.keys() {
            if let Some(record) = self.get(*id) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_email(&self, value: &str) -> Vec<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        __ids.iter().filter_map(|&__id| self.get(__id)).collect()
    }
    pub fn find_by_email_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Vec<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.email);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn get_by_email(&self, value: &str) -> Option<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = self.email_index.get(&__k)?;
        __ids.iter().find_map(|&__id| self.get(__id))
    }
    pub fn get_by_email_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Option<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return None,
        };
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.email);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    return Some(__rec);
                }
            }
        }
        None
    }
    pub fn export_live_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        for &row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(row).unwrap_or(true) {
                indices.push(row);
            }
        }
        indices.sort_unstable();
        indices
    }
    pub fn export_col_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.id_col.export(indices)
    }
    pub fn export_col_created_at(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.created_at_col.export(indices)
    }
    pub fn __with_scan<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&UserScanRef<'_>) -> bool,
        f: impl FnOnce(&mut Vec<UserScanRef<'_>>) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __UserScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
            email_col: forgedb_storage::BufferedVariableColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __UserScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            email_col: self
                .email_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<UserScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__slot)
                .expect("Failed to read string");
            let email_value = __bufs
                .email_col
                .read_str(__slot)
                .expect("Failed to read string");
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let __row_ref = UserScanRef {
                __slot,
                id: id_value,
                name: name_value,
                email: email_value,
                created_at: created_at_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        f(&mut __refs)
    }
    pub fn __with_page<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&UserScanRef<'_>) -> bool,
        sort: impl FnOnce(&mut Vec<UserScanRef<'_>>),
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[UserPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __UserPageScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
            email_col: forgedb_storage::BufferedVariableColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __UserPageScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            email_col: self
                .email_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<UserScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__slot)
                .expect("Failed to read string");
            let email_value = __bufs
                .email_col
                .read_str(__slot)
                .expect("Failed to read string");
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let __row_ref = UserScanRef {
                __slot,
                id: id_value,
                name: name_value,
                email: email_value,
                created_at: created_at_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        let __total = __refs.len();
        sort(&mut __refs);
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        let __page = &__refs[__start..__end];
        let __page_rows: Vec<usize> = __page
            .iter()
            .map(|__r| __rows[__r.__slot])
            .collect();
        #[allow(non_camel_case_types)]
        struct __UserPageBufs {}
        let __page_bufs = __UserPageBufs {};
        let mut __views: Vec<UserPageRef<'_>> = Vec::with_capacity(__page.len());
        for (__pslot, __ref) in __page.iter().enumerate() {
            __views
                .push(UserPageRef {
                    id: __ref.id,
                    name: __ref.name,
                    email: __ref.email,
                    created_at: __ref.created_at,
                    posts: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __with_fast_page<R>(
        &self,
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[UserPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = {
            let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
            __all.sort_unstable();
            self.tombstones
                .live_indices(&__all)
                .expect("Failed to read tombstone liveness")
        };
        let __total = __rows.len();
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        #[allow(non_camel_case_types)]
        struct __UserFastPageBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
            email_col: forgedb_storage::BufferedVariableColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __UserFastPageBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            email_col: self
                .email_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
        };
        let mut __views: Vec<UserPageRef<'_>> = Vec::with_capacity(__end - __start);
        for __pslot in 0..(__end - __start) {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let email_value = __bufs
                .email_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__pslot)
                    .expect("Failed to read from column"),
            );
            __views
                .push(UserPageRef {
                    id: id_value,
                    name: name_value,
                    email: email_value,
                    created_at: created_at_value,
                    posts: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __rows_by_email(&self, value: &str) -> Option<Vec<usize>> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return Some(Vec::new()),
        };
        let mut __out = Vec::with_capacity(__ids.len());
        for &__id in __ids {
            if let Some(&__row) = self.id_to_row.get(&__id) {
                __out.push(__row);
            }
        }
        Some(__out)
    }
    pub fn reader(&self) -> UserStorageReader {
        UserStorageReader {
            id_to_row: self.id_to_row.clone(),
            id_versions: self.id_versions.clone(),
            id_col: self.id_col.reader().expect("Failed to open column reader"),
            name_col: self.name_col.reader().expect("Failed to open column reader"),
            email_col: self.email_col.reader().expect("Failed to open column reader"),
            created_at_col: self
                .created_at_col
                .reader()
                .expect("Failed to open column reader"),
            email_index: self.email_index.clone(),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open tombstones reader"),
        }
    }
}
pub struct UserStorageReader {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    id_col: forgedb_storage::FixedColumnReader,
    name_col: forgedb_storage::VariableColumnReader,
    email_col: forgedb_storage::VariableColumnReader,
    created_at_col: forgedb_storage::FixedColumnReader,
    email_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::TombstonesReader,
}
impl UserStorageReader {
    pub fn read_at(&self, row_index: usize) -> Option<User> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let name_value = self
            .name_col
            .read_string(row_index)
            .expect("Failed to read string");
        let email_value = self
            .email_col
            .read_string(row_index)
            .expect("Failed to read string");
        let created_at_value = Timestamp::from(
            self
                .created_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        Some(User {
            id: id_value,
            name: name_value,
            email: email_value,
            created_at: created_at_value,
            posts: (),
        })
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<User> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<User> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_email_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Vec<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.email);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn get_by_email_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Option<User> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.email_index.get(&__k) {
            Some(__s) => __s,
            None => return None,
        };
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.email);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    return Some(__rec);
                }
            }
        }
        None
    }
}
fn validate_user(record: &User) -> Result<(), ValidationError> {
    {
        let __v = &record.name;
        let __len = __v.chars().count();
        if __len < 1i64 as usize || __len > 100i64 as usize {
            return Err(ValidationError::Constraint {
                model: "User",
                field: "name",
                rule: "length",
                message: "length must be between 1 and 100".to_string(),
            });
        }
    }
    {
        let __v = &record.email;
        let __parts: Vec<&str> = __v.split('@').collect();
        let __ok = __parts.len() == 2 && !__parts[0].is_empty()
            && __parts[1].contains('.') && !__parts[1].starts_with('.')
            && !__parts[1].ends_with('.') && !__v.chars().any(|__c| __c.is_whitespace());
        if !__ok {
            return Err(ValidationError::Constraint {
                model: "User",
                field: "email",
                rule: "email",
                message: "must be a valid email address".to_string(),
            });
        }
    }
    Ok(())
}
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Post {
    #[serde(default)]
    pub id: Uuid,
    pub title: String,
    pub views: u64,
    pub published: bool,
    pub author: Uuid,
    #[schema(value_type = String)]
    #[serde(default = "__forgedb_default_ts")]
    pub created_at: Timestamp,
    pub tags: (),
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostAgg {
    pub id: Uuid,
    pub views: u64,
    pub published: bool,
}
#[derive(Debug, Clone)]
pub struct PostScanRef<'a> {
    pub __slot: usize,
    pub id: Uuid,
    pub title: &'a str,
    pub views: u64,
    pub published: bool,
    pub created_at: Timestamp,
}
#[derive(serde::Serialize)]
pub struct PostPageRef<'a> {
    pub id: Uuid,
    pub title: &'a str,
    pub views: u64,
    pub published: bool,
    pub author: Uuid,
    pub created_at: Timestamp,
    pub tags: (),
}
pub struct PostStorage {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    row_count: usize,
    id_col: FixedColumn,
    title_col: VariableColumn,
    views_col: FixedColumn,
    published_col: FixedColumn,
    author_col: FixedColumn,
    created_at_col: FixedColumn,
    views_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    author_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    views_ordered: std::sync::Arc<
        std::collections::BTreeMap<u64, std::collections::BTreeSet<Uuid>>,
    >,
    tombstones: forgedb_storage::Tombstones,
    wal: forgedb_wal::WalManager,
    writes_since_checkpoint: u64,
    root: std::path::PathBuf,
    dead_since_compaction: u64,
    in_transaction: bool,
    checkpoint_deferred: bool,
    compact_deferred: bool,
    compaction_due: bool,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl PostStorage {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("post/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            title_col: VariableColumn::new(
                    root.join("post/variable/string_data_1.bin"),
                    root.join("post/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            views_col: FixedColumn::new(root.join("post/fixed/u64_2.bin"), 8usize)
                .expect("Failed to create fixed column"),
            published_col: FixedColumn::new(root.join("post/fixed/bool_3.bin"), 1usize)
                .expect("Failed to create fixed column"),
            author_col: FixedColumn::new(root.join("post/fixed/uuid_4.bin"), 16usize)
                .expect("Failed to create fixed column"),
            created_at_col: FixedColumn::new(
                    root.join("post/fixed/timestamp_5.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            views_index: std::sync::Arc::new(std::collections::HashMap::new()),
            author_index: std::sync::Arc::new(std::collections::HashMap::new()),
            views_ordered: std::sync::Arc::new(std::collections::BTreeMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("post/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("post/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let n = db.tombstones.len();
        db.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = db.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut db.id_versions).entry(id).or_default().push(i);
        }
        let __ids: Vec<Uuid> = db.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match db.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if db.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let views_value = db
                .views_col
                .read_u64(__row)
                .expect("Failed to read from column");
            let author_value = {
                let bytes = db
                    .author_col
                    .read_uuid(__row)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            {
                let __k: String = {
                    {
                        let __v = &(views_value);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut db.views_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                let __k: String = {
                    {
                        let __v = &(author_value);
                        let mut __buf = [0u8; 36];
                        let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                        let mut __k = String::with_capacity(1 + __s.len());
                        __k.push('\u{1}');
                        __k.push_str(__s);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut db.author_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                std::sync::Arc::make_mut(&mut db.views_ordered)
                    .entry(views_value)
                    .or_default()
                    .insert(__id);
            }
        }
        let _ = db.write_manifest(root);
        db
    }
    fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("post/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            title_col: VariableColumn::new(
                    root.join("post/variable/string_data_1.bin"),
                    root.join("post/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            views_col: FixedColumn::new(root.join("post/fixed/u64_2.bin"), 8usize)
                .expect("Failed to create fixed column"),
            published_col: FixedColumn::new(root.join("post/fixed/bool_3.bin"), 1usize)
                .expect("Failed to create fixed column"),
            author_col: FixedColumn::new(root.join("post/fixed/uuid_4.bin"), 16usize)
                .expect("Failed to create fixed column"),
            created_at_col: FixedColumn::new(
                    root.join("post/fixed/timestamp_5.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            views_index: std::sync::Arc::new(std::collections::HashMap::new()),
            author_index: std::sync::Arc::new(std::collections::HashMap::new()),
            views_ordered: std::sync::Arc::new(std::collections::BTreeMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("post/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("post/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let _ = db.write_manifest(root);
        db
    }
    fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "id".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_0.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "title".to_string(), column_type : forgedb_storage::ColumnType::String,
            column_index : 1usize, value_size : 0usize, kind :
            forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_1.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "views".to_string(), column_type : forgedb_storage::ColumnType::U64,
            column_index : 2usize, value_size : 8usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u64_2.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "published"
            .to_string(), column_type : forgedb_storage::ColumnType::Bool, column_index :
            3usize, value_size : 1usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/bool_3.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "author".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 4usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_4.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "created_at".to_string(), column_type :
            forgedb_storage::ColumnType::Timestamp, column_index : 5usize, value_size :
            8usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/timestamp_5.bin".to_string(), }
        ];
        let __manifest_abs = root.join("post/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.auto_sequences)
            .unwrap_or_default();
        #[allow(unused_mut)]
        let mut __auto_sequences = __persisted_seqs;
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "tombstones.bin".to_string(),
                bytes_per_row: 1usize,
            }),
            auto_sequences: __auto_sequences,
        };
        manifest.save_to(&__manifest_abs)
    }
    fn bump_compaction_epoch(&self, root: &std::path::Path) {
        let __manifest_abs = root.join("post/manifest.json");
        if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
            __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
            let _ = __m.save_to(&__manifest_abs);
        }
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    pub fn insert(&mut self, record: Post) -> Result<Uuid, ValidationError> {
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        let row_index = self.row_count;
        let id = record.id;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Post", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.title_col.append_string(&record.title).expect("Failed to append string");
        self.views_col.append_u64(record.views).expect("Failed to append to column");
        self.published_col
            .append_bool(record.published)
            .expect("Failed to append to column");
        self.author_col
            .append_uuid(*record.author.as_bytes())
            .expect("Failed to append to column");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.views);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.views_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            let __k: String = {
                {
                    let __v = &(record.author);
                    let mut __buf = [0u8; 36];
                    let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                    let mut __k = String::with_capacity(1 + __s.len());
                    __k.push('\u{1}');
                    __k.push_str(__s);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.author_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            std::sync::Arc::make_mut(&mut self.views_ordered)
                .entry(record.views)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Post", row_index, forgedb_changefeed::ChangeKind::Inserted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Post",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Inserted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        Ok(id)
    }
    pub fn update(&mut self, id: Uuid, record: Post) -> Result<bool, ValidationError> {
        if !self.id_to_row.contains_key(&id) {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        let __old = self.get(id);
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Post", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.title_col.append_string(&record.title).expect("Failed to append string");
        self.views_col.append_u64(record.views).expect("Failed to append to column");
        self.published_col
            .append_bool(record.published)
            .expect("Failed to append to column");
        self.author_col
            .append_uuid(*record.author.as_bytes())
            .expect("Failed to append to column");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(__old_rec) = &__old {
            {
                let __k: String = {
                    {
                        let __v = &(__old_rec.views);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.views_index);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
            {
                let __k: String = {
                    {
                        let __v = &(__old_rec.author);
                        let mut __buf = [0u8; 36];
                        let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                        let mut __k = String::with_capacity(1 + __s.len());
                        __k.push('\u{1}');
                        __k.push_str(__s);
                        __k
                    }
                };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.author_index);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
            {
                let __ok = { __old_rec.views };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.views_ordered);
                if let Some(__set) = __map.get_mut(&__ok) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__ok);
                }
            }
        }
        {
            let __k: String = {
                {
                    let __v = &(record.views);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.views_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            let __k: String = {
                {
                    let __v = &(record.author);
                    let mut __buf = [0u8; 36];
                    let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                    let mut __k = String::with_capacity(1 + __s.len());
                    __k.push('\u{1}');
                    __k.push_str(__s);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.author_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            std::sync::Arc::make_mut(&mut self.views_ordered)
                .entry(record.views)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Post", row_index, forgedb_changefeed::ChangeKind::Updated);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Post",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Updated,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        Ok(true)
    }
    pub fn delete(&mut self, id: Uuid) -> bool {
        let record = match self.get(id) {
            Some(r) => r,
            None => return false,
        };
        let deleted_row = *self
            .id_to_row
            .get(&id)
            .expect("id present: get succeeded above");
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(1u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Post", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.title_col.append_string(&record.title).expect("Failed to append string");
        self.views_col.append_u64(record.views).expect("Failed to append to column");
        self.published_col
            .append_bool(record.published)
            .expect("Failed to append to column");
        self.author_col
            .append_uuid(*record.author.as_bytes())
            .expect("Failed to append to column");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(true).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.views);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.views_index);
            if let Some(__set) = __map.get_mut(&__k) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__k);
            }
        }
        {
            let __k: String = {
                {
                    let __v = &(record.author);
                    let mut __buf = [0u8; 36];
                    let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                    let mut __k = String::with_capacity(1 + __s.len());
                    __k.push('\u{1}');
                    __k.push_str(__s);
                    __k
                }
            };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.author_index);
            if let Some(__set) = __map.get_mut(&__k) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__k);
            }
        }
        {
            let __ok = { record.views };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.views_ordered);
            if let Some(__set) = __map.get_mut(&__ok) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__ok);
            }
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Post", deleted_row, forgedb_changefeed::ChangeKind::Deleted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Post",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        true
    }
    pub fn apply(
        &mut self,
        kind: forgedb_changefeed::ChangeKind,
        bytes: &[u8],
    ) -> Result<(), ApplyError> {
        match kind {
            forgedb_changefeed::ChangeKind::Inserted => {
                let record: Post = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                self.insert(record)?;
            }
            forgedb_changefeed::ChangeKind::Updated => {
                let record: Post = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.update(id, record)?;
            }
            forgedb_changefeed::ChangeKind::Deleted => {
                let record: Post = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.delete(id);
            }
            forgedb_changefeed::ChangeKind::Linked => {}
        }
        Ok(())
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.id_col.sync_to_drive()?;
        self.title_col.sync_to_drive()?;
        self.views_col.sync_to_drive()?;
        self.published_col.sync_to_drive()?;
        self.author_col.sync_to_drive()?;
        self.created_at_col.sync_to_drive()?;
        self.tombstones.sync_to_drive()?;
        self.tombstones.barrier()?;
        Ok(())
    }
    fn recover_from_wal(&mut self) {
        let __anchor = self.tombstones.len();
        while self.id_col.len() < __anchor {
            self.id_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.title_col.len() < __anchor {
            self.title_col.append_string("").expect("Failed to backfill string column");
        }
        while self.views_col.len() < __anchor {
            self.views_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.published_col.len() < __anchor {
            self.published_col.append_bool(false).expect("Failed to backfill column");
        }
        while self.author_col.len() < __anchor {
            self.author_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.created_at_col.len() < __anchor {
            self.created_at_col
                .append_timestamp(0i64)
                .expect("Failed to backfill column");
        }
        self.id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.title_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.views_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.published_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.author_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.created_at_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.row_count = __anchor;
        let __entries = self
            .wal
            .replay(|_| -> std::io::Result<()> { Ok(()) })
            .expect("Failed to replay WAL on recovery");
        for __entry in &__entries {
            if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                if payload.len() < 9 {
                    continue;
                }
                let mut __ri = [0u8; 8];
                __ri.copy_from_slice(&payload[0..8]);
                let __row_index = u64::from_le_bytes(__ri) as usize;
                if __row_index < self.row_count {
                    continue;
                }
                let __deleted = payload[8] != 0;
                let record: Post = match serde_json::from_slice(&payload[9..]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                self.id_col
                    .append_uuid(*record.id.as_bytes())
                    .expect("Failed to append to column");
                self.title_col
                    .append_string(&record.title)
                    .expect("Failed to append string");
                self.views_col
                    .append_u64(record.views)
                    .expect("Failed to append to column");
                self.published_col
                    .append_bool(record.published)
                    .expect("Failed to append to column");
                self.author_col
                    .append_uuid(*record.author.as_bytes())
                    .expect("Failed to append to column");
                self.created_at_col
                    .append_timestamp(i64::from(record.created_at))
                    .expect("Failed to append to column");
                self.tombstones
                    .append(__deleted)
                    .expect("Failed to append tombstone on recovery");
                self.row_count += 1;
            }
        }
    }
    pub fn checkpoint(&mut self) {
        self.id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.title_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.views_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.published_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.author_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.created_at_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync tombstones to drive on checkpoint");
        self.tombstones.barrier().expect("Failed to issue checkpoint device barrier");
        self.wal.truncate().expect("Failed to truncate WAL on checkpoint");
        self.writes_since_checkpoint = 0;
    }
    pub fn compact(&mut self) {
        if self.in_transaction {
            self.compact_deferred = true;
            return;
        }
        self.checkpoint();
        let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
        for &__row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                __keep.push(__row);
            }
        }
        let __config = forgedb_compaction::CompactionConfig::default();
        let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
        if __compactor.compact_model_keeping("post", &__keep).is_ok() {
            let mut __keep_sorted = __keep;
            __keep_sorted.sort_unstable();
            let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
            let __saved_views_index = std::sync::Arc::clone(&self.views_index);
            let __saved_author_index = std::sync::Arc::clone(&self.author_index);
            let __saved_views_ordered = std::sync::Arc::clone(&self.views_ordered);
            let __root = self.root.clone();
            let __feed = self.changefeed.take();
            let __broker = self.broker.take();
            *self = Self::new_at_no_rehydrate(&__root);
            self.changefeed = __feed;
            self.broker = __broker;
            self.views_index = __saved_views_index;
            self.author_index = __saved_author_index;
            self.views_ordered = __saved_views_ordered;
            let mut __new_id_to_row: std::collections::HashMap<_, usize> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            for (&__id, &__old_row) in __old_id_to_row.iter() {
                if __keep_sorted.binary_search(&__old_row).is_ok() {
                    let __new_row = __keep_sorted
                        .partition_point(|&__r| __r < __old_row);
                    __new_id_to_row.insert(__id, __new_row);
                    __new_id_versions.insert(__id, vec![__new_row]);
                }
            }
            self.id_to_row = std::sync::Arc::new(__new_id_to_row);
            self.id_versions = std::sync::Arc::new(__new_id_versions);
            self.bump_compaction_epoch(&__root);
        }
        self.dead_since_compaction = 0;
        self.compaction_due = false;
    }
    pub fn maintain(&mut self) {
        if self.compaction_due && !self.in_transaction {
            self.compaction_due = false;
            self.compact();
        }
    }
    pub fn __reindex_committed(&mut self) {
        std::sync::Arc::make_mut(&mut self.id_to_row).clear();
        std::sync::Arc::make_mut(&mut self.id_versions).clear();
        std::sync::Arc::make_mut(&mut self.views_index).clear();
        std::sync::Arc::make_mut(&mut self.author_index).clear();
        let n = self.tombstones.len();
        self.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = self.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(i);
        }
        let __ids: Vec<Uuid> = self.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match self.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if self.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let views_value = self
                .views_col
                .read_u64(__row)
                .expect("Failed to read from column");
            let author_value = {
                let bytes = self
                    .author_col
                    .read_uuid(__row)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            {
                let __k: String = {
                    {
                        let __v = &(views_value);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut self.views_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                let __k: String = {
                    {
                        let __v = &(author_value);
                        let mut __buf = [0u8; 36];
                        let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                        let mut __k = String::with_capacity(1 + __s.len());
                        __k.push('\u{1}');
                        __k.push_str(__s);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut self.author_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                std::sync::Arc::make_mut(&mut self.views_ordered)
                    .entry(views_value)
                    .or_default()
                    .insert(__id);
            }
        }
    }
    pub fn __reindex_delta(&mut self, from: usize) {
        let __n = self.row_count;
        for __r in from..__n {
            let id = {
                let bytes = self
                    .id_col
                    .read_uuid(__r)
                    .expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            if let Some(__old_rec) = self.get(id) {
                {
                    let __k: String = {
                        {
                            let __v = &(__old_rec.views);
                            use std::fmt::Write as _;
                            let mut __k = String::from('\u{2}');
                            let _ = write!(__k, "{}", __v);
                            __k
                        }
                    };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.views_index);
                    if let Some(__set) = __map.get_mut(&__k) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__k);
                    }
                }
                {
                    let __k: String = {
                        {
                            let __v = &(__old_rec.author);
                            let mut __buf = [0u8; 36];
                            let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                            let mut __k = String::with_capacity(1 + __s.len());
                            __k.push('\u{1}');
                            __k.push_str(__s);
                            __k
                        }
                    };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.author_index);
                    if let Some(__set) = __map.get_mut(&__k) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__k);
                    }
                }
                {
                    let __ok = { __old_rec.views };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.views_ordered);
                    if let Some(__set) = __map.get_mut(&__ok) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__ok);
                    }
                }
            }
            let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(__r);
            if !__deleted {
                if let Some(__new_rec) = self.read_at(__r) {
                    {
                        let __k: String = {
                            {
                                let __v = &(__new_rec.views);
                                use std::fmt::Write as _;
                                let mut __k = String::from('\u{2}');
                                let _ = write!(__k, "{}", __v);
                                __k
                            }
                        };
                        std::sync::Arc::make_mut(&mut self.views_index)
                            .entry(__k)
                            .or_default()
                            .insert(id);
                    }
                    {
                        let __k: String = {
                            {
                                let __v = &(__new_rec.author);
                                let mut __buf = [0u8; 36];
                                let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                                let mut __k = String::with_capacity(1 + __s.len());
                                __k.push('\u{1}');
                                __k.push_str(__s);
                                __k
                            }
                        };
                        std::sync::Arc::make_mut(&mut self.author_index)
                            .entry(__k)
                            .or_default()
                            .insert(id);
                    }
                    {
                        std::sync::Arc::make_mut(&mut self.views_ordered)
                            .entry(__new_rec.views)
                            .or_default()
                            .insert(id);
                    }
                }
            }
        }
    }
    pub fn __sync_columns_from_disk(&mut self) {
        let _ = self.id_col.sync_from_disk();
        let _ = self.title_col.sync_from_disk();
        let _ = self.views_col.sync_from_disk();
        let _ = self.published_col.sync_from_disk();
        let _ = self.author_col.sync_from_disk();
        let _ = self.created_at_col.sync_from_disk();
        let _ = self.tombstones.sync_from_disk();
        self.row_count = self.tombstones.len();
    }
    pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
        self.id_col.truncate_to_rows(rows)?;
        self.title_col.truncate_to_rows(rows)?;
        self.views_col.truncate_to_rows(rows)?;
        self.published_col.truncate_to_rows(rows)?;
        self.author_col.truncate_to_rows(rows)?;
        self.created_at_col.truncate_to_rows(rows)?;
        self.tombstones.truncate_to_rows(rows)?;
        Ok(())
    }
    pub fn __recover_to_committed(&mut self, committed_len: usize) {
        if committed_len >= self.row_count {
            return;
        }
        self.id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.title_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.views_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.published_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.author_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.created_at_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.tombstones
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate tombstones on journal recovery");
        self.row_count = committed_len;
        self.wal.truncate().expect("Failed to truncate WAL on journal recovery");
        let __root = self.root.clone();
        let __feed = self.changefeed.take();
        let __broker = self.broker.take();
        *self = Self::new_at(&__root);
        self.changefeed = __feed;
        self.broker = __broker;
    }
    pub fn __stage_append(&mut self, record: Post, deleted: bool) -> usize {
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        if deleted {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(1u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Post", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        } else {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(0u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Post", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.title_col.append_string(&record.title).expect("Failed to append string");
        self.views_col.append_u64(record.views).expect("Failed to append to column");
        self.published_col
            .append_bool(record.published)
            .expect("Failed to append to column");
        self.author_col
            .append_uuid(*record.author.as_bytes())
            .expect("Failed to append to column");
        self.created_at_col
            .append_timestamp(i64::from(record.created_at))
            .expect("Failed to append to column");
        self.tombstones.append(deleted).expect("Failed to append tombstone");
        self.row_count += 1;
        row_index
    }
    pub fn run_deferred_maintenance(&mut self) {
        if self.compact_deferred {
            self.compact_deferred = false;
            self.checkpoint_deferred = false;
            self.compact();
        } else if self.checkpoint_deferred {
            self.checkpoint_deferred = false;
            self.checkpoint();
        }
    }
    pub fn read_at(&self, row_index: usize) -> Option<Post> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let title_value = self
            .title_col
            .read_string(row_index)
            .expect("Failed to read string");
        let views_value = self
            .views_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let published_value = self
            .published_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        let author_value = {
            let bytes = self
                .author_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let created_at_value = Timestamp::from(
            self
                .created_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        Some(Post {
            id: id_value,
            title: title_value,
            views: views_value,
            published: published_value,
            author: author_value,
            created_at: created_at_value,
            tags: (),
        })
    }
    pub fn get(&self, id: Uuid) -> Option<Post> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_at(row_index)
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Post> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Post> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn all(&self) -> Vec<Post> {
        let mut records = Vec::new();
        for id in self.id_to_row.keys() {
            if let Some(record) = self.get(*id) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_views(&self, value: u64) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.views_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        __ids.iter().filter_map(|&__id| self.get(__id)).collect()
    }
    pub fn find_by_views_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: u64,
    ) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.views_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.views);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn find_by_author(&self, value: Uuid) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __buf = [0u8; 36];
                let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                let mut __k = String::with_capacity(1 + __s.len());
                __k.push('\u{1}');
                __k.push_str(__s);
                __k
            }
        };
        let __ids = match self.author_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        __ids.iter().filter_map(|&__id| self.get(__id)).collect()
    }
    pub fn find_by_author_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: Uuid,
    ) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __buf = [0u8; 36];
                let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                let mut __k = String::with_capacity(1 + __s.len());
                __k.push('\u{1}');
                __k.push_str(__s);
                __k
            }
        };
        let __ids = match self.author_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.author);
                        let mut __buf = [0u8; 36];
                        let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                        let mut __k = String::with_capacity(1 + __s.len());
                        __k.push('\u{1}');
                        __k.push_str(__s);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn find_by_views_range(
        &self,
        min: Option<u64>,
        max: Option<u64>,
        descending: bool,
        limit: Option<usize>,
    ) -> Vec<Post> {
        let __lo_v: Option<u64> = min.map(|__v| __v);
        let __hi_v: Option<u64> = max.map(|__v| __v);
        if let (Some(__l), Some(__h)) = (__lo_v.as_ref(), __hi_v.as_ref()) {
            if __l > __h {
                return Vec::new();
            }
        }
        let __lo = match __lo_v {
            Some(__v) => std::ops::Bound::Included(__v),
            None => std::ops::Bound::Unbounded,
        };
        let __hi = match __hi_v {
            Some(__v) => std::ops::Bound::Included(__v),
            None => std::ops::Bound::Unbounded,
        };
        let mut __out: Vec<Post> = Vec::new();
        let __iter: Box<
            dyn Iterator<Item = (&u64, &std::collections::BTreeSet<Uuid>)> + '_,
        > = if descending {
            Box::new(self.views_ordered.range((__lo, __hi)).rev())
        } else {
            Box::new(self.views_ordered.range((__lo, __hi)))
        };
        'outer: for (_k, __ids) in __iter {
            for &__id in __ids {
                if let Some(__r) = self.get(__id) {
                    __out.push(__r);
                    if let Some(__n) = limit {
                        if __out.len() >= __n {
                            break 'outer;
                        }
                    }
                }
            }
        }
        __out
    }
    pub fn export_live_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        for &row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(row).unwrap_or(true) {
                indices.push(row);
            }
        }
        indices.sort_unstable();
        indices
    }
    pub fn export_col_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.id_col.export(indices)
    }
    pub fn export_col_views(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.views_col.export(indices)
    }
    pub fn export_col_author(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.author_col.export(indices)
    }
    pub fn export_col_created_at(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.created_at_col.export(indices)
    }
    fn __proj_resolve_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<usize> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        Some(versions[pos - 1])
    }
    fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
        let watermark = snap.watermark();
        let mut rows = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            rows.push(versions[pos - 1]);
        }
        rows
    }
    pub fn read_agg_at(&self, row_index: usize) -> Option<PostAgg> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let views_value = self
            .views_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let published_value = self
            .published_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        Some(PostAgg {
            id: id_value,
            views: views_value,
            published: published_value,
        })
    }
    pub fn get_agg(&self, id: Uuid) -> Option<PostAgg> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_agg_at(row_index)
    }
    pub fn all_agg(&self) -> Vec<PostAgg> {
        let mut __rows: Vec<usize> = self.id_to_row.values().copied().collect();
        __rows.sort_unstable();
        let __rows = self
            .tombstones
            .live_indices(&__rows)
            .expect("Failed to read tombstone liveness");
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __PostAggScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            views_col: forgedb_storage::BufferedFixedColumn,
            published_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __PostAggScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            views_col: self
                .views_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            published_col: self
                .published_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut rows = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let views_value = __bufs
                .views_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let published_value = __bufs
                .published_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            rows.push(PostAgg {
                id: id_value,
                views: views_value,
                published: published_value,
            });
        }
        rows
    }
    pub fn get_agg_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<PostAgg> {
        let row_index = self.__proj_resolve_at(snap, id)?;
        self.read_agg_at(row_index)
    }
    pub fn all_agg_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<PostAgg> {
        let mut records = Vec::new();
        for row_index in self.__proj_live_rows_at(snap) {
            if let Some(record) = self.read_agg_at(row_index) {
                records.push(record);
            }
        }
        records
    }
    pub fn __with_scan<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&PostScanRef<'_>) -> bool,
        f: impl FnOnce(&mut Vec<PostScanRef<'_>>) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __PostScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            title_col: forgedb_storage::BufferedVariableColumn,
            views_col: forgedb_storage::BufferedFixedColumn,
            published_col: forgedb_storage::BufferedFixedColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __PostScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            title_col: self
                .title_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            views_col: self
                .views_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            published_col: self
                .published_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<PostScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let title_value = __bufs
                .title_col
                .read_str(__slot)
                .expect("Failed to read string");
            let views_value = __bufs
                .views_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let published_value = __bufs
                .published_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let __row_ref = PostScanRef {
                __slot,
                id: id_value,
                title: title_value,
                views: views_value,
                published: published_value,
                created_at: created_at_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        f(&mut __refs)
    }
    pub fn __with_page<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&PostScanRef<'_>) -> bool,
        sort: impl FnOnce(&mut Vec<PostScanRef<'_>>),
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[PostPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __PostPageScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            title_col: forgedb_storage::BufferedVariableColumn,
            views_col: forgedb_storage::BufferedFixedColumn,
            published_col: forgedb_storage::BufferedFixedColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __PostPageScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            title_col: self
                .title_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            views_col: self
                .views_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            published_col: self
                .published_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<PostScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let title_value = __bufs
                .title_col
                .read_str(__slot)
                .expect("Failed to read string");
            let views_value = __bufs
                .views_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let published_value = __bufs
                .published_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let __row_ref = PostScanRef {
                __slot,
                id: id_value,
                title: title_value,
                views: views_value,
                published: published_value,
                created_at: created_at_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        let __total = __refs.len();
        sort(&mut __refs);
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        let __page = &__refs[__start..__end];
        let __page_rows: Vec<usize> = __page
            .iter()
            .map(|__r| __rows[__r.__slot])
            .collect();
        #[allow(non_camel_case_types)]
        struct __PostPageBufs {
            author_col: forgedb_storage::BufferedFixedColumn,
        }
        let __page_bufs = __PostPageBufs {
            author_col: self
                .author_col
                .gather_buffered(&__page_rows)
                .expect("Failed to bulk-load page column"),
        };
        let mut __views: Vec<PostPageRef<'_>> = Vec::with_capacity(__page.len());
        for (__pslot, __ref) in __page.iter().enumerate() {
            let author_value = {
                let bytes = __page_bufs
                    .author_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            __views
                .push(PostPageRef {
                    id: __ref.id,
                    title: __ref.title,
                    views: __ref.views,
                    published: __ref.published,
                    author: author_value,
                    created_at: __ref.created_at,
                    tags: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __with_fast_page<R>(
        &self,
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[PostPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = {
            let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
            __all.sort_unstable();
            self.tombstones
                .live_indices(&__all)
                .expect("Failed to read tombstone liveness")
        };
        let __total = __rows.len();
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        #[allow(non_camel_case_types)]
        struct __PostFastPageBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            title_col: forgedb_storage::BufferedVariableColumn,
            views_col: forgedb_storage::BufferedFixedColumn,
            published_col: forgedb_storage::BufferedFixedColumn,
            author_col: forgedb_storage::BufferedFixedColumn,
            created_at_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __PostFastPageBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            title_col: self
                .title_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            views_col: self
                .views_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            published_col: self
                .published_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            author_col: self
                .author_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            created_at_col: self
                .created_at_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
        };
        let mut __views: Vec<PostPageRef<'_>> = Vec::with_capacity(__end - __start);
        for __pslot in 0..(__end - __start) {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let title_value = __bufs
                .title_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let views_value = __bufs
                .views_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let published_value = __bufs
                .published_col
                .read_bool(__pslot)
                .expect("Failed to read from column");
            let author_value = {
                let bytes = __bufs
                    .author_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let created_at_value = Timestamp::from(
                __bufs
                    .created_at_col
                    .read_timestamp(__pslot)
                    .expect("Failed to read from column"),
            );
            __views
                .push(PostPageRef {
                    id: id_value,
                    title: title_value,
                    views: views_value,
                    published: published_value,
                    author: author_value,
                    created_at: created_at_value,
                    tags: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __rows_by_views(&self, value: &str) -> Option<Vec<usize>> {
        let __typed = value.parse::<u64>().ok()?;
        let __k: String = {
            {
                let __v = &(__typed);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.views_index.get(&__k) {
            Some(__s) => __s,
            None => return Some(Vec::new()),
        };
        let mut __out = Vec::with_capacity(__ids.len());
        for &__id in __ids {
            if let Some(&__row) = self.id_to_row.get(&__id) {
                __out.push(__row);
            }
        }
        Some(__out)
    }
    pub fn reader(&self) -> PostStorageReader {
        PostStorageReader {
            id_to_row: self.id_to_row.clone(),
            id_versions: self.id_versions.clone(),
            id_col: self.id_col.reader().expect("Failed to open column reader"),
            title_col: self.title_col.reader().expect("Failed to open column reader"),
            views_col: self.views_col.reader().expect("Failed to open column reader"),
            published_col: self
                .published_col
                .reader()
                .expect("Failed to open column reader"),
            author_col: self.author_col.reader().expect("Failed to open column reader"),
            created_at_col: self
                .created_at_col
                .reader()
                .expect("Failed to open column reader"),
            views_index: self.views_index.clone(),
            author_index: self.author_index.clone(),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open tombstones reader"),
        }
    }
}
pub struct PostStorageReader {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    id_col: forgedb_storage::FixedColumnReader,
    title_col: forgedb_storage::VariableColumnReader,
    views_col: forgedb_storage::FixedColumnReader,
    published_col: forgedb_storage::FixedColumnReader,
    author_col: forgedb_storage::FixedColumnReader,
    created_at_col: forgedb_storage::FixedColumnReader,
    views_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    author_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::TombstonesReader,
}
impl PostStorageReader {
    pub fn read_at(&self, row_index: usize) -> Option<Post> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let title_value = self
            .title_col
            .read_string(row_index)
            .expect("Failed to read string");
        let views_value = self
            .views_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let published_value = self
            .published_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        let author_value = {
            let bytes = self
                .author_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let created_at_value = Timestamp::from(
            self
                .created_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        Some(Post {
            id: id_value,
            title: title_value,
            views: views_value,
            published: published_value,
            author: author_value,
            created_at: created_at_value,
            tags: (),
        })
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Post> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Post> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_views_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: u64,
    ) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.views_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.views);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn find_by_author_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: Uuid,
    ) -> Vec<Post> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __buf = [0u8; 36];
                let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                let mut __k = String::with_capacity(1 + __s.len());
                __k.push('\u{1}');
                __k.push_str(__s);
                __k
            }
        };
        let __ids = match self.author_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.author);
                        let mut __buf = [0u8; 36];
                        let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                        let mut __k = String::with_capacity(1 + __s.len());
                        __k.push('\u{1}');
                        __k.push_str(__s);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
}
fn validate_post(record: &Post) -> Result<(), ValidationError> {
    {
        let __v = &record.title;
        let __len = __v.chars().count();
        if __len < 1i64 as usize || __len > 200i64 as usize {
            return Err(ValidationError::Constraint {
                model: "Post",
                field: "title",
                rule: "length",
                message: "length must be between 1 and 200".to_string(),
            });
        }
    }
    Ok(())
}
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tag {
    #[serde(default)]
    pub id: Uuid,
    pub name: String,
    pub posts: (),
}
#[derive(Debug, Clone)]
pub struct TagScanRef<'a> {
    pub __slot: usize,
    pub id: Uuid,
    pub name: &'a str,
}
#[derive(serde::Serialize)]
pub struct TagPageRef<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub posts: (),
}
pub struct TagStorage {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    row_count: usize,
    id_col: FixedColumn,
    name_col: VariableColumn,
    name_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::Tombstones,
    wal: forgedb_wal::WalManager,
    writes_since_checkpoint: u64,
    root: std::path::PathBuf,
    dead_since_compaction: u64,
    in_transaction: bool,
    checkpoint_deferred: bool,
    compact_deferred: bool,
    compaction_due: bool,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl TagStorage {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("tag/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            name_col: VariableColumn::new(
                    root.join("tag/variable/string_data_1.bin"),
                    root.join("tag/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            name_index: std::sync::Arc::new(std::collections::HashMap::new()),
            tombstones: forgedb_storage::Tombstones::new(root.join("tag/tombstones.bin"))
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("tag/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let n = db.tombstones.len();
        db.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = db.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut db.id_versions).entry(id).or_default().push(i);
        }
        let __ids: Vec<Uuid> = db.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match db.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if db.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let name_value = db
                .name_col
                .read_string(__row)
                .expect("Failed to read string");
            {
                let __k: String = {
                    {
                        let __v = &(name_value);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut db.name_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
        }
        let _ = db.write_manifest(root);
        db
    }
    fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("tag/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            name_col: VariableColumn::new(
                    root.join("tag/variable/string_data_1.bin"),
                    root.join("tag/variable/string_offsets_1.bin"),
                )
                .expect("Failed to create variable column"),
            name_index: std::sync::Arc::new(std::collections::HashMap::new()),
            tombstones: forgedb_storage::Tombstones::new(root.join("tag/tombstones.bin"))
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("tag/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let _ = db.write_manifest(root);
        db
    }
    fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "id".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_0.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "name".to_string(), column_type : forgedb_storage::ColumnType::String,
            column_index : 1usize, value_size : 0usize, kind :
            forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_1.bin".to_string(), }
        ];
        let __manifest_abs = root.join("tag/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.auto_sequences)
            .unwrap_or_default();
        #[allow(unused_mut)]
        let mut __auto_sequences = __persisted_seqs;
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "tombstones.bin".to_string(),
                bytes_per_row: 1usize,
            }),
            auto_sequences: __auto_sequences,
        };
        manifest.save_to(&__manifest_abs)
    }
    fn bump_compaction_epoch(&self, root: &std::path::Path) {
        let __manifest_abs = root.join("tag/manifest.json");
        if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
            __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
            let _ = __m.save_to(&__manifest_abs);
        }
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    pub fn insert(&mut self, record: Tag) -> Result<Uuid, ValidationError> {
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.name_index.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                return Err(ValidationError::Unique {
                    model: "Tag",
                    field: "name",
                });
            }
        }
        let row_index = self.row_count;
        let id = record.id;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Tag", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.name_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Tag", row_index, forgedb_changefeed::ChangeKind::Inserted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Tag",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Inserted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        Ok(id)
    }
    pub fn update(&mut self, id: Uuid, record: Tag) -> Result<bool, ValidationError> {
        if !self.id_to_row.contains_key(&id) {
            return Ok(false);
        }
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if let Some(__ids) = self.name_index.get(&__uk) {
                if __ids.iter().any(|__i| *__i != id) {
                    return Err(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    });
                }
            }
        }
        let __old = self.get(id);
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Tag", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(__old_rec) = &__old {
            {
                let __k: String = {
                    {
                        let __v = &(__old_rec.name);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.name_index);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
        }
        {
            let __k: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.name_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Tag", row_index, forgedb_changefeed::ChangeKind::Updated);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Tag",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Updated,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        Ok(true)
    }
    pub fn delete(&mut self, id: Uuid) -> bool {
        let record = match self.get(id) {
            Some(r) => r,
            None => return false,
        };
        let deleted_row = *self
            .id_to_row
            .get(&id)
            .expect("id present: get succeeded above");
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(1u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Tag", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.tombstones.append(true).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.name_index);
            if let Some(__set) = __map.get_mut(&__k) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__k);
            }
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Tag", deleted_row, forgedb_changefeed::ChangeKind::Deleted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Tag",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        true
    }
    pub fn apply(
        &mut self,
        kind: forgedb_changefeed::ChangeKind,
        bytes: &[u8],
    ) -> Result<(), ApplyError> {
        match kind {
            forgedb_changefeed::ChangeKind::Inserted => {
                let record: Tag = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                self.insert(record)?;
            }
            forgedb_changefeed::ChangeKind::Updated => {
                let record: Tag = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.update(id, record)?;
            }
            forgedb_changefeed::ChangeKind::Deleted => {
                let record: Tag = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.delete(id);
            }
            forgedb_changefeed::ChangeKind::Linked => {}
        }
        Ok(())
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.id_col.sync_to_drive()?;
        self.name_col.sync_to_drive()?;
        self.tombstones.sync_to_drive()?;
        self.tombstones.barrier()?;
        Ok(())
    }
    fn recover_from_wal(&mut self) {
        let __anchor = self.tombstones.len();
        while self.id_col.len() < __anchor {
            self.id_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.name_col.len() < __anchor {
            self.name_col.append_string("").expect("Failed to backfill string column");
        }
        self.id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.name_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.row_count = __anchor;
        let __entries = self
            .wal
            .replay(|_| -> std::io::Result<()> { Ok(()) })
            .expect("Failed to replay WAL on recovery");
        for __entry in &__entries {
            if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                if payload.len() < 9 {
                    continue;
                }
                let mut __ri = [0u8; 8];
                __ri.copy_from_slice(&payload[0..8]);
                let __row_index = u64::from_le_bytes(__ri) as usize;
                if __row_index < self.row_count {
                    continue;
                }
                let __deleted = payload[8] != 0;
                let record: Tag = match serde_json::from_slice(&payload[9..]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                self.id_col
                    .append_uuid(*record.id.as_bytes())
                    .expect("Failed to append to column");
                self.name_col
                    .append_string(&record.name)
                    .expect("Failed to append string");
                self.tombstones
                    .append(__deleted)
                    .expect("Failed to append tombstone on recovery");
                self.row_count += 1;
            }
        }
    }
    pub fn checkpoint(&mut self) {
        self.id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.name_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync tombstones to drive on checkpoint");
        self.tombstones.barrier().expect("Failed to issue checkpoint device barrier");
        self.wal.truncate().expect("Failed to truncate WAL on checkpoint");
        self.writes_since_checkpoint = 0;
    }
    pub fn compact(&mut self) {
        if self.in_transaction {
            self.compact_deferred = true;
            return;
        }
        self.checkpoint();
        let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
        for &__row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                __keep.push(__row);
            }
        }
        let __config = forgedb_compaction::CompactionConfig::default();
        let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
        if __compactor.compact_model_keeping("tag", &__keep).is_ok() {
            let mut __keep_sorted = __keep;
            __keep_sorted.sort_unstable();
            let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
            let __saved_name_index = std::sync::Arc::clone(&self.name_index);
            let __root = self.root.clone();
            let __feed = self.changefeed.take();
            let __broker = self.broker.take();
            *self = Self::new_at_no_rehydrate(&__root);
            self.changefeed = __feed;
            self.broker = __broker;
            self.name_index = __saved_name_index;
            let mut __new_id_to_row: std::collections::HashMap<_, usize> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            for (&__id, &__old_row) in __old_id_to_row.iter() {
                if __keep_sorted.binary_search(&__old_row).is_ok() {
                    let __new_row = __keep_sorted
                        .partition_point(|&__r| __r < __old_row);
                    __new_id_to_row.insert(__id, __new_row);
                    __new_id_versions.insert(__id, vec![__new_row]);
                }
            }
            self.id_to_row = std::sync::Arc::new(__new_id_to_row);
            self.id_versions = std::sync::Arc::new(__new_id_versions);
            self.bump_compaction_epoch(&__root);
        }
        self.dead_since_compaction = 0;
        self.compaction_due = false;
    }
    pub fn maintain(&mut self) {
        if self.compaction_due && !self.in_transaction {
            self.compaction_due = false;
            self.compact();
        }
    }
    pub fn __reindex_committed(&mut self) {
        std::sync::Arc::make_mut(&mut self.id_to_row).clear();
        std::sync::Arc::make_mut(&mut self.id_versions).clear();
        std::sync::Arc::make_mut(&mut self.name_index).clear();
        let n = self.tombstones.len();
        self.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = self.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(i);
        }
        let __ids: Vec<Uuid> = self.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match self.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if self.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let name_value = self
                .name_col
                .read_string(__row)
                .expect("Failed to read string");
            {
                let __k: String = {
                    {
                        let __v = &(name_value);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut self.name_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
        }
    }
    pub fn __reindex_delta(&mut self, from: usize) {
        let __n = self.row_count;
        for __r in from..__n {
            let id = {
                let bytes = self
                    .id_col
                    .read_uuid(__r)
                    .expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            if let Some(__old_rec) = self.get(id) {
                {
                    let __k: String = {
                        {
                            let __v = &(__old_rec.name);
                            let mut __k = String::with_capacity(1 + __v.len());
                            __k.push('\u{1}');
                            __k.push_str(__v);
                            __k
                        }
                    };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.name_index);
                    if let Some(__set) = __map.get_mut(&__k) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__k);
                    }
                }
            }
            let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(__r);
            if !__deleted {
                if let Some(__new_rec) = self.read_at(__r) {
                    {
                        let __k: String = {
                            {
                                let __v = &(__new_rec.name);
                                let mut __k = String::with_capacity(1 + __v.len());
                                __k.push('\u{1}');
                                __k.push_str(__v);
                                __k
                            }
                        };
                        std::sync::Arc::make_mut(&mut self.name_index)
                            .entry(__k)
                            .or_default()
                            .insert(id);
                    }
                }
            }
        }
    }
    pub fn __sync_columns_from_disk(&mut self) {
        let _ = self.id_col.sync_from_disk();
        let _ = self.name_col.sync_from_disk();
        let _ = self.tombstones.sync_from_disk();
        self.row_count = self.tombstones.len();
    }
    pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
        self.id_col.truncate_to_rows(rows)?;
        self.name_col.truncate_to_rows(rows)?;
        self.tombstones.truncate_to_rows(rows)?;
        Ok(())
    }
    pub fn __recover_to_committed(&mut self, committed_len: usize) {
        if committed_len >= self.row_count {
            return;
        }
        self.id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.name_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.tombstones
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate tombstones on journal recovery");
        self.row_count = committed_len;
        self.wal.truncate().expect("Failed to truncate WAL on journal recovery");
        let __root = self.root.clone();
        let __feed = self.changefeed.take();
        let __broker = self.broker.take();
        *self = Self::new_at(&__root);
        self.changefeed = __feed;
        self.broker = __broker;
    }
    pub fn __stage_append(&mut self, record: Tag, deleted: bool) -> usize {
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        if deleted {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(1u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Tag", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        } else {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(0u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Tag", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.name_col.append_string(&record.name).expect("Failed to append string");
        self.tombstones.append(deleted).expect("Failed to append tombstone");
        self.row_count += 1;
        row_index
    }
    pub fn run_deferred_maintenance(&mut self) {
        if self.compact_deferred {
            self.compact_deferred = false;
            self.checkpoint_deferred = false;
            self.compact();
        } else if self.checkpoint_deferred {
            self.checkpoint_deferred = false;
            self.checkpoint();
        }
    }
    pub fn read_at(&self, row_index: usize) -> Option<Tag> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let name_value = self
            .name_col
            .read_string(row_index)
            .expect("Failed to read string");
        Some(Tag {
            id: id_value,
            name: name_value,
            posts: (),
        })
    }
    pub fn get(&self, id: Uuid) -> Option<Tag> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_at(row_index)
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Tag> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Tag> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn all(&self) -> Vec<Tag> {
        let mut records = Vec::new();
        for id in self.id_to_row.keys() {
            if let Some(record) = self.get(*id) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_name(&self, value: &str) -> Vec<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        __ids.iter().filter_map(|&__id| self.get(__id)).collect()
    }
    pub fn find_by_name_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Vec<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.name);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn get_by_name(&self, value: &str) -> Option<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = self.name_index.get(&__k)?;
        __ids.iter().find_map(|&__id| self.get(__id))
    }
    pub fn get_by_name_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Option<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return None,
        };
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.name);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    return Some(__rec);
                }
            }
        }
        None
    }
    pub fn export_live_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        for &row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(row).unwrap_or(true) {
                indices.push(row);
            }
        }
        indices.sort_unstable();
        indices
    }
    pub fn export_col_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.id_col.export(indices)
    }
    pub fn __with_scan<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&TagScanRef<'_>) -> bool,
        f: impl FnOnce(&mut Vec<TagScanRef<'_>>) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __TagScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __TagScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<TagScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__slot)
                .expect("Failed to read string");
            let __row_ref = TagScanRef {
                __slot,
                id: id_value,
                name: name_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        f(&mut __refs)
    }
    pub fn __with_page<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&TagScanRef<'_>) -> bool,
        sort: impl FnOnce(&mut Vec<TagScanRef<'_>>),
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[TagPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __TagPageScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __TagPageScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<TagScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__slot)
                .expect("Failed to read string");
            let __row_ref = TagScanRef {
                __slot,
                id: id_value,
                name: name_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        let __total = __refs.len();
        sort(&mut __refs);
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        let __page = &__refs[__start..__end];
        let __page_rows: Vec<usize> = __page
            .iter()
            .map(|__r| __rows[__r.__slot])
            .collect();
        #[allow(non_camel_case_types)]
        struct __TagPageBufs {}
        let __page_bufs = __TagPageBufs {};
        let mut __views: Vec<TagPageRef<'_>> = Vec::with_capacity(__page.len());
        for (__pslot, __ref) in __page.iter().enumerate() {
            __views
                .push(TagPageRef {
                    id: __ref.id,
                    name: __ref.name,
                    posts: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __with_fast_page<R>(
        &self,
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[TagPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = {
            let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
            __all.sort_unstable();
            self.tombstones
                .live_indices(&__all)
                .expect("Failed to read tombstone liveness")
        };
        let __total = __rows.len();
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        #[allow(non_camel_case_types)]
        struct __TagFastPageBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            name_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __TagFastPageBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            name_col: self
                .name_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
        };
        let mut __views: Vec<TagPageRef<'_>> = Vec::with_capacity(__end - __start);
        for __pslot in 0..(__end - __start) {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let name_value = __bufs
                .name_col
                .read_str(__pslot)
                .expect("Failed to read string");
            __views
                .push(TagPageRef {
                    id: id_value,
                    name: name_value,
                    posts: (),
                });
        }
        f(__total, &__views)
    }
    pub fn __rows_by_name(&self, value: &str) -> Option<Vec<usize>> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return Some(Vec::new()),
        };
        let mut __out = Vec::with_capacity(__ids.len());
        for &__id in __ids {
            if let Some(&__row) = self.id_to_row.get(&__id) {
                __out.push(__row);
            }
        }
        Some(__out)
    }
    pub fn reader(&self) -> TagStorageReader {
        TagStorageReader {
            id_to_row: self.id_to_row.clone(),
            id_versions: self.id_versions.clone(),
            id_col: self.id_col.reader().expect("Failed to open column reader"),
            name_col: self.name_col.reader().expect("Failed to open column reader"),
            name_index: self.name_index.clone(),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open tombstones reader"),
        }
    }
}
pub struct TagStorageReader {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    id_col: forgedb_storage::FixedColumnReader,
    name_col: forgedb_storage::VariableColumnReader,
    name_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::TombstonesReader,
}
impl TagStorageReader {
    pub fn read_at(&self, row_index: usize) -> Option<Tag> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let name_value = self
            .name_col
            .read_string(row_index)
            .expect("Failed to read string");
        Some(Tag {
            id: id_value,
            name: name_value,
            posts: (),
        })
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Tag> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Tag> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_name_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Vec<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.name);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn get_by_name_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: &str,
    ) -> Option<Tag> {
        let __k: String = {
            {
                let __v = &(value);
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            }
        };
        let __ids = match self.name_index.get(&__k) {
            Some(__s) => __s,
            None => return None,
        };
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.name);
                        let mut __k = String::with_capacity(1 + __v.len());
                        __k.push('\u{1}');
                        __k.push_str(__v);
                        __k
                    }
                };
                if __rk == __k {
                    return Some(__rec);
                }
            }
        }
        None
    }
}
fn validate_tag(record: &Tag) -> Result<(), ValidationError> {
    {
        let __v = &record.name;
        let __len = __v.chars().count();
        if __len < 1i64 as usize || __len > 50i64 as usize {
            return Err(ValidationError::Constraint {
                model: "Tag",
                field: "name",
                rule: "length",
                message: "length must be between 1 and 50".to_string(),
            });
        }
    }
    Ok(())
}
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Metric {
    #[serde(default)]
    pub id: Uuid,
    #[schema(value_type = String)]
    #[serde(default = "__forgedb_default_ts")]
    pub recorded_at: Timestamp,
    pub device_id: u64,
    pub sample_seq: u64,
    pub region: u32,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub disk_pct: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub req_count: u64,
    pub err_count: u64,
    pub p50_micros: u32,
    pub p95_micros: u32,
    pub p99_micros: u32,
    pub queue_depth: u32,
    pub open_conns: u32,
    pub gc_pause_micros: u32,
    pub uptime_secs: i64,
    pub temp_celsius: f64,
    pub throttled: bool,
    pub healthy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricHot {
    pub id: Uuid,
    pub cpu_pct: f64,
    pub mem_pct: f64,
}
#[derive(Debug, Clone)]
pub struct MetricScanRef<'a> {
    pub __slot: usize,
    pub id: Uuid,
    pub recorded_at: Timestamp,
    pub device_id: u64,
    pub sample_seq: u64,
    pub region: u32,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub disk_pct: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub req_count: u64,
    pub err_count: u64,
    pub p50_micros: u32,
    pub p95_micros: u32,
    pub p99_micros: u32,
    pub queue_depth: u32,
    pub open_conns: u32,
    pub gc_pause_micros: u32,
    pub uptime_secs: i64,
    pub temp_celsius: f64,
    pub throttled: bool,
    pub healthy: bool,
    pub __borrow: ::std::marker::PhantomData<&'a ()>,
}
#[derive(serde::Serialize)]
pub struct MetricPageRef<'a> {
    pub id: Uuid,
    pub recorded_at: Timestamp,
    pub device_id: u64,
    pub sample_seq: u64,
    pub region: u32,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub disk_pct: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub req_count: u64,
    pub err_count: u64,
    pub p50_micros: u32,
    pub p95_micros: u32,
    pub p99_micros: u32,
    pub queue_depth: u32,
    pub open_conns: u32,
    pub gc_pause_micros: u32,
    pub uptime_secs: i64,
    pub temp_celsius: f64,
    pub throttled: bool,
    pub healthy: bool,
    #[serde(skip)]
    pub __borrow: ::std::marker::PhantomData<&'a ()>,
}
pub struct MetricStorage {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    row_count: usize,
    id_col: FixedColumn,
    recorded_at_col: FixedColumn,
    device_id_col: FixedColumn,
    sample_seq_col: FixedColumn,
    region_col: FixedColumn,
    cpu_pct_col: FixedColumn,
    mem_pct_col: FixedColumn,
    disk_pct_col: FixedColumn,
    net_rx_bytes_col: FixedColumn,
    net_tx_bytes_col: FixedColumn,
    req_count_col: FixedColumn,
    err_count_col: FixedColumn,
    p50_micros_col: FixedColumn,
    p95_micros_col: FixedColumn,
    p99_micros_col: FixedColumn,
    queue_depth_col: FixedColumn,
    open_conns_col: FixedColumn,
    gc_pause_micros_col: FixedColumn,
    uptime_secs_col: FixedColumn,
    temp_celsius_col: FixedColumn,
    throttled_col: FixedColumn,
    healthy_col: FixedColumn,
    device_id_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    device_id_ordered: std::sync::Arc<
        std::collections::BTreeMap<u64, std::collections::BTreeSet<Uuid>>,
    >,
    tombstones: forgedb_storage::Tombstones,
    wal: forgedb_wal::WalManager,
    writes_since_checkpoint: u64,
    root: std::path::PathBuf,
    dead_since_compaction: u64,
    in_transaction: bool,
    checkpoint_deferred: bool,
    compact_deferred: bool,
    compaction_due: bool,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl MetricStorage {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("metric/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            recorded_at_col: FixedColumn::new(
                    root.join("metric/fixed/timestamp_1.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            device_id_col: FixedColumn::new(root.join("metric/fixed/u64_2.bin"), 8usize)
                .expect("Failed to create fixed column"),
            sample_seq_col: FixedColumn::new(root.join("metric/fixed/u64_3.bin"), 8usize)
                .expect("Failed to create fixed column"),
            region_col: FixedColumn::new(root.join("metric/fixed/u32_4.bin"), 4usize)
                .expect("Failed to create fixed column"),
            cpu_pct_col: FixedColumn::new(root.join("metric/fixed/f64_5.bin"), 8usize)
                .expect("Failed to create fixed column"),
            mem_pct_col: FixedColumn::new(root.join("metric/fixed/f64_6.bin"), 8usize)
                .expect("Failed to create fixed column"),
            disk_pct_col: FixedColumn::new(root.join("metric/fixed/f64_7.bin"), 8usize)
                .expect("Failed to create fixed column"),
            net_rx_bytes_col: FixedColumn::new(
                    root.join("metric/fixed/u64_8.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            net_tx_bytes_col: FixedColumn::new(
                    root.join("metric/fixed/u64_9.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            req_count_col: FixedColumn::new(root.join("metric/fixed/u64_10.bin"), 8usize)
                .expect("Failed to create fixed column"),
            err_count_col: FixedColumn::new(root.join("metric/fixed/u64_11.bin"), 8usize)
                .expect("Failed to create fixed column"),
            p50_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_12.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            p95_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_13.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            p99_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_14.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            queue_depth_col: FixedColumn::new(
                    root.join("metric/fixed/u32_15.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            open_conns_col: FixedColumn::new(
                    root.join("metric/fixed/u32_16.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            gc_pause_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_17.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            uptime_secs_col: FixedColumn::new(
                    root.join("metric/fixed/i64_18.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            temp_celsius_col: FixedColumn::new(
                    root.join("metric/fixed/f64_19.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            throttled_col: FixedColumn::new(
                    root.join("metric/fixed/bool_20.bin"),
                    1usize,
                )
                .expect("Failed to create fixed column"),
            healthy_col: FixedColumn::new(root.join("metric/fixed/bool_21.bin"), 1usize)
                .expect("Failed to create fixed column"),
            device_id_index: std::sync::Arc::new(std::collections::HashMap::new()),
            device_id_ordered: std::sync::Arc::new(std::collections::BTreeMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("metric/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("metric/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let n = db.tombstones.len();
        db.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = db.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut db.id_versions).entry(id).or_default().push(i);
        }
        let __ids: Vec<Uuid> = db.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match db.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if db.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let device_id_value = db
                .device_id_col
                .read_u64(__row)
                .expect("Failed to read from column");
            {
                let __k: String = {
                    {
                        let __v = &(device_id_value);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut db.device_id_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                std::sync::Arc::make_mut(&mut db.device_id_ordered)
                    .entry(device_id_value)
                    .or_default()
                    .insert(__id);
            }
        }
        let _ = db.write_manifest(root);
        db
    }
    fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("metric/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            recorded_at_col: FixedColumn::new(
                    root.join("metric/fixed/timestamp_1.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            device_id_col: FixedColumn::new(root.join("metric/fixed/u64_2.bin"), 8usize)
                .expect("Failed to create fixed column"),
            sample_seq_col: FixedColumn::new(root.join("metric/fixed/u64_3.bin"), 8usize)
                .expect("Failed to create fixed column"),
            region_col: FixedColumn::new(root.join("metric/fixed/u32_4.bin"), 4usize)
                .expect("Failed to create fixed column"),
            cpu_pct_col: FixedColumn::new(root.join("metric/fixed/f64_5.bin"), 8usize)
                .expect("Failed to create fixed column"),
            mem_pct_col: FixedColumn::new(root.join("metric/fixed/f64_6.bin"), 8usize)
                .expect("Failed to create fixed column"),
            disk_pct_col: FixedColumn::new(root.join("metric/fixed/f64_7.bin"), 8usize)
                .expect("Failed to create fixed column"),
            net_rx_bytes_col: FixedColumn::new(
                    root.join("metric/fixed/u64_8.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            net_tx_bytes_col: FixedColumn::new(
                    root.join("metric/fixed/u64_9.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            req_count_col: FixedColumn::new(root.join("metric/fixed/u64_10.bin"), 8usize)
                .expect("Failed to create fixed column"),
            err_count_col: FixedColumn::new(root.join("metric/fixed/u64_11.bin"), 8usize)
                .expect("Failed to create fixed column"),
            p50_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_12.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            p95_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_13.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            p99_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_14.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            queue_depth_col: FixedColumn::new(
                    root.join("metric/fixed/u32_15.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            open_conns_col: FixedColumn::new(
                    root.join("metric/fixed/u32_16.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            gc_pause_micros_col: FixedColumn::new(
                    root.join("metric/fixed/u32_17.bin"),
                    4usize,
                )
                .expect("Failed to create fixed column"),
            uptime_secs_col: FixedColumn::new(
                    root.join("metric/fixed/i64_18.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            temp_celsius_col: FixedColumn::new(
                    root.join("metric/fixed/f64_19.bin"),
                    8usize,
                )
                .expect("Failed to create fixed column"),
            throttled_col: FixedColumn::new(
                    root.join("metric/fixed/bool_20.bin"),
                    1usize,
                )
                .expect("Failed to create fixed column"),
            healthy_col: FixedColumn::new(root.join("metric/fixed/bool_21.bin"), 1usize)
                .expect("Failed to create fixed column"),
            device_id_index: std::sync::Arc::new(std::collections::HashMap::new()),
            device_id_ordered: std::sync::Arc::new(std::collections::BTreeMap::new()),
            tombstones: forgedb_storage::Tombstones::new(
                    root.join("metric/tombstones.bin"),
                )
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("metric/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let _ = db.write_manifest(root);
        db
    }
    fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "id".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_0.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "recorded_at".to_string(), column_type :
            forgedb_storage::ColumnType::Timestamp, column_index : 1usize, value_size :
            8usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/timestamp_1.bin".to_string(), }, forgedb_storage::ColumnMetadata {
            name : "device_id".to_string(), column_type :
            forgedb_storage::ColumnType::U64, column_index : 2usize, value_size : 8usize,
            kind : forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u64_2.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "sample_seq"
            .to_string(), column_type : forgedb_storage::ColumnType::U64, column_index :
            3usize, value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/u64_3.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "region".to_string(), column_type :
            forgedb_storage::ColumnType::U32, column_index : 4usize, value_size : 4usize,
            kind : forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u32_4.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "cpu_pct"
            .to_string(), column_type : forgedb_storage::ColumnType::F64, column_index :
            5usize, value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/f64_5.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "mem_pct".to_string(), column_type :
            forgedb_storage::ColumnType::F64, column_index : 6usize, value_size : 8usize,
            kind : forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/f64_6.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "disk_pct"
            .to_string(), column_type : forgedb_storage::ColumnType::F64, column_index :
            7usize, value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/f64_7.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "net_rx_bytes".to_string(),
            column_type : forgedb_storage::ColumnType::U64, column_index : 8usize,
            value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path
            : "fixed/u64_8.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "net_tx_bytes".to_string(), column_type : forgedb_storage::ColumnType::U64,
            column_index : 9usize, value_size : 8usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u64_9.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "req_count"
            .to_string(), column_type : forgedb_storage::ColumnType::U64, column_index :
            10usize, value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/u64_10.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "err_count".to_string(), column_type
            : forgedb_storage::ColumnType::U64, column_index : 11usize, value_size :
            8usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/u64_11.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "p50_micros".to_string(), column_type : forgedb_storage::ColumnType::U32,
            column_index : 12usize, value_size : 4usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u32_12.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "p95_micros"
            .to_string(), column_type : forgedb_storage::ColumnType::U32, column_index :
            13usize, value_size : 4usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/u32_13.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "p99_micros".to_string(),
            column_type : forgedb_storage::ColumnType::U32, column_index : 14usize,
            value_size : 4usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path
            : "fixed/u32_14.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "queue_depth".to_string(), column_type : forgedb_storage::ColumnType::U32,
            column_index : 15usize, value_size : 4usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u32_15.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "open_conns"
            .to_string(), column_type : forgedb_storage::ColumnType::U32, column_index :
            16usize, value_size : 4usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/u32_16.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "gc_pause_micros".to_string(),
            column_type : forgedb_storage::ColumnType::U32, column_index : 17usize,
            value_size : 4usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path
            : "fixed/u32_17.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "uptime_secs".to_string(), column_type : forgedb_storage::ColumnType::I64,
            column_index : 18usize, value_size : 8usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/i64_18.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "temp_celsius"
            .to_string(), column_type : forgedb_storage::ColumnType::F64, column_index :
            19usize, value_size : 8usize, kind : forgedb_storage::ColumnKind::Fixed,
            relative_path : "fixed/f64_19.bin".to_string(), },
            forgedb_storage::ColumnMetadata { name : "throttled".to_string(), column_type
            : forgedb_storage::ColumnType::Bool, column_index : 20usize, value_size :
            1usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/bool_20.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "healthy".to_string(), column_type : forgedb_storage::ColumnType::Bool,
            column_index : 21usize, value_size : 1usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/bool_21.bin"
            .to_string(), }
        ];
        let __manifest_abs = root.join("metric/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.auto_sequences)
            .unwrap_or_default();
        #[allow(unused_mut)]
        let mut __auto_sequences = __persisted_seqs;
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "tombstones.bin".to_string(),
                bytes_per_row: 1usize,
            }),
            auto_sequences: __auto_sequences,
        };
        manifest.save_to(&__manifest_abs)
    }
    fn bump_compaction_epoch(&self, root: &std::path::Path) {
        let __manifest_abs = root.join("metric/manifest.json");
        if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
            __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
            let _ = __m.save_to(&__manifest_abs);
        }
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    pub fn insert(&mut self, record: Metric) -> Result<Uuid, ValidationError> {
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let row_index = self.row_count;
        let id = record.id;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Metric", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.recorded_at_col
            .append_timestamp(i64::from(record.recorded_at))
            .expect("Failed to append to column");
        self.device_id_col
            .append_u64(record.device_id)
            .expect("Failed to append to column");
        self.sample_seq_col
            .append_u64(record.sample_seq)
            .expect("Failed to append to column");
        self.region_col.append_u32(record.region).expect("Failed to append to column");
        self.cpu_pct_col.append_f64(record.cpu_pct).expect("Failed to append to column");
        self.mem_pct_col.append_f64(record.mem_pct).expect("Failed to append to column");
        self.disk_pct_col
            .append_f64(record.disk_pct)
            .expect("Failed to append to column");
        self.net_rx_bytes_col
            .append_u64(record.net_rx_bytes)
            .expect("Failed to append to column");
        self.net_tx_bytes_col
            .append_u64(record.net_tx_bytes)
            .expect("Failed to append to column");
        self.req_count_col
            .append_u64(record.req_count)
            .expect("Failed to append to column");
        self.err_count_col
            .append_u64(record.err_count)
            .expect("Failed to append to column");
        self.p50_micros_col
            .append_u32(record.p50_micros)
            .expect("Failed to append to column");
        self.p95_micros_col
            .append_u32(record.p95_micros)
            .expect("Failed to append to column");
        self.p99_micros_col
            .append_u32(record.p99_micros)
            .expect("Failed to append to column");
        self.queue_depth_col
            .append_u32(record.queue_depth)
            .expect("Failed to append to column");
        self.open_conns_col
            .append_u32(record.open_conns)
            .expect("Failed to append to column");
        self.gc_pause_micros_col
            .append_u32(record.gc_pause_micros)
            .expect("Failed to append to column");
        self.uptime_secs_col
            .append_i64(record.uptime_secs)
            .expect("Failed to append to column");
        self.temp_celsius_col
            .append_f64(record.temp_celsius)
            .expect("Failed to append to column");
        self.throttled_col
            .append_bool(record.throttled)
            .expect("Failed to append to column");
        self.healthy_col
            .append_bool(record.healthy)
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.device_id);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.device_id_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            std::sync::Arc::make_mut(&mut self.device_id_ordered)
                .entry(record.device_id)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Metric", row_index, forgedb_changefeed::ChangeKind::Inserted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Metric",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Inserted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        Ok(id)
    }
    pub fn update(&mut self, id: Uuid, record: Metric) -> Result<bool, ValidationError> {
        if !self.id_to_row.contains_key(&id) {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let __old = self.get(id);
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Metric", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.recorded_at_col
            .append_timestamp(i64::from(record.recorded_at))
            .expect("Failed to append to column");
        self.device_id_col
            .append_u64(record.device_id)
            .expect("Failed to append to column");
        self.sample_seq_col
            .append_u64(record.sample_seq)
            .expect("Failed to append to column");
        self.region_col.append_u32(record.region).expect("Failed to append to column");
        self.cpu_pct_col.append_f64(record.cpu_pct).expect("Failed to append to column");
        self.mem_pct_col.append_f64(record.mem_pct).expect("Failed to append to column");
        self.disk_pct_col
            .append_f64(record.disk_pct)
            .expect("Failed to append to column");
        self.net_rx_bytes_col
            .append_u64(record.net_rx_bytes)
            .expect("Failed to append to column");
        self.net_tx_bytes_col
            .append_u64(record.net_tx_bytes)
            .expect("Failed to append to column");
        self.req_count_col
            .append_u64(record.req_count)
            .expect("Failed to append to column");
        self.err_count_col
            .append_u64(record.err_count)
            .expect("Failed to append to column");
        self.p50_micros_col
            .append_u32(record.p50_micros)
            .expect("Failed to append to column");
        self.p95_micros_col
            .append_u32(record.p95_micros)
            .expect("Failed to append to column");
        self.p99_micros_col
            .append_u32(record.p99_micros)
            .expect("Failed to append to column");
        self.queue_depth_col
            .append_u32(record.queue_depth)
            .expect("Failed to append to column");
        self.open_conns_col
            .append_u32(record.open_conns)
            .expect("Failed to append to column");
        self.gc_pause_micros_col
            .append_u32(record.gc_pause_micros)
            .expect("Failed to append to column");
        self.uptime_secs_col
            .append_i64(record.uptime_secs)
            .expect("Failed to append to column");
        self.temp_celsius_col
            .append_f64(record.temp_celsius)
            .expect("Failed to append to column");
        self.throttled_col
            .append_bool(record.throttled)
            .expect("Failed to append to column");
        self.healthy_col
            .append_bool(record.healthy)
            .expect("Failed to append to column");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(__old_rec) = &__old {
            {
                let __k: String = {
                    {
                        let __v = &(__old_rec.device_id);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.device_id_index);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
            {
                let __ok = { __old_rec.device_id };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut self.device_id_ordered);
                if let Some(__set) = __map.get_mut(&__ok) {
                    __set.remove(&(id));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__ok);
                }
            }
        }
        {
            let __k: String = {
                {
                    let __v = &(record.device_id);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            std::sync::Arc::make_mut(&mut self.device_id_index)
                .entry(__k)
                .or_default()
                .insert(id);
        }
        {
            std::sync::Arc::make_mut(&mut self.device_id_ordered)
                .entry(record.device_id)
                .or_default()
                .insert(id);
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Metric", row_index, forgedb_changefeed::ChangeKind::Updated);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Metric",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Updated,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        Ok(true)
    }
    pub fn delete(&mut self, id: Uuid) -> bool {
        let record = match self.get(id) {
            Some(r) => r,
            None => return false,
        };
        let deleted_row = *self
            .id_to_row
            .get(&id)
            .expect("id present: get succeeded above");
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(1u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Metric", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.recorded_at_col
            .append_timestamp(i64::from(record.recorded_at))
            .expect("Failed to append to column");
        self.device_id_col
            .append_u64(record.device_id)
            .expect("Failed to append to column");
        self.sample_seq_col
            .append_u64(record.sample_seq)
            .expect("Failed to append to column");
        self.region_col.append_u32(record.region).expect("Failed to append to column");
        self.cpu_pct_col.append_f64(record.cpu_pct).expect("Failed to append to column");
        self.mem_pct_col.append_f64(record.mem_pct).expect("Failed to append to column");
        self.disk_pct_col
            .append_f64(record.disk_pct)
            .expect("Failed to append to column");
        self.net_rx_bytes_col
            .append_u64(record.net_rx_bytes)
            .expect("Failed to append to column");
        self.net_tx_bytes_col
            .append_u64(record.net_tx_bytes)
            .expect("Failed to append to column");
        self.req_count_col
            .append_u64(record.req_count)
            .expect("Failed to append to column");
        self.err_count_col
            .append_u64(record.err_count)
            .expect("Failed to append to column");
        self.p50_micros_col
            .append_u32(record.p50_micros)
            .expect("Failed to append to column");
        self.p95_micros_col
            .append_u32(record.p95_micros)
            .expect("Failed to append to column");
        self.p99_micros_col
            .append_u32(record.p99_micros)
            .expect("Failed to append to column");
        self.queue_depth_col
            .append_u32(record.queue_depth)
            .expect("Failed to append to column");
        self.open_conns_col
            .append_u32(record.open_conns)
            .expect("Failed to append to column");
        self.gc_pause_micros_col
            .append_u32(record.gc_pause_micros)
            .expect("Failed to append to column");
        self.uptime_secs_col
            .append_i64(record.uptime_secs)
            .expect("Failed to append to column");
        self.temp_celsius_col
            .append_f64(record.temp_celsius)
            .expect("Failed to append to column");
        self.throttled_col
            .append_bool(record.throttled)
            .expect("Failed to append to column");
        self.healthy_col
            .append_bool(record.healthy)
            .expect("Failed to append to column");
        self.tombstones.append(true).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        {
            let __k: String = {
                {
                    let __v = &(record.device_id);
                    use std::fmt::Write as _;
                    let mut __k = String::from('\u{2}');
                    let _ = write!(__k, "{}", __v);
                    __k
                }
            };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.device_id_index);
            if let Some(__set) = __map.get_mut(&__k) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__k);
            }
        }
        {
            let __ok = { record.device_id };
            let mut __empty = false;
            let __map = std::sync::Arc::make_mut(&mut self.device_id_ordered);
            if let Some(__set) = __map.get_mut(&__ok) {
                __set.remove(&(id));
                __empty = __set.is_empty();
            }
            if __empty {
                __map.remove(&__ok);
            }
        }
        if let Some(feed) = &self.changefeed {
            feed.emit("Metric", deleted_row, forgedb_changefeed::ChangeKind::Deleted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Metric",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        true
    }
    pub fn apply(
        &mut self,
        kind: forgedb_changefeed::ChangeKind,
        bytes: &[u8],
    ) -> Result<(), ApplyError> {
        match kind {
            forgedb_changefeed::ChangeKind::Inserted => {
                let record: Metric = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                self.insert(record)?;
            }
            forgedb_changefeed::ChangeKind::Updated => {
                let record: Metric = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.update(id, record)?;
            }
            forgedb_changefeed::ChangeKind::Deleted => {
                let record: Metric = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.delete(id);
            }
            forgedb_changefeed::ChangeKind::Linked => {}
        }
        Ok(())
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.id_col.sync_to_drive()?;
        self.recorded_at_col.sync_to_drive()?;
        self.device_id_col.sync_to_drive()?;
        self.sample_seq_col.sync_to_drive()?;
        self.region_col.sync_to_drive()?;
        self.cpu_pct_col.sync_to_drive()?;
        self.mem_pct_col.sync_to_drive()?;
        self.disk_pct_col.sync_to_drive()?;
        self.net_rx_bytes_col.sync_to_drive()?;
        self.net_tx_bytes_col.sync_to_drive()?;
        self.req_count_col.sync_to_drive()?;
        self.err_count_col.sync_to_drive()?;
        self.p50_micros_col.sync_to_drive()?;
        self.p95_micros_col.sync_to_drive()?;
        self.p99_micros_col.sync_to_drive()?;
        self.queue_depth_col.sync_to_drive()?;
        self.open_conns_col.sync_to_drive()?;
        self.gc_pause_micros_col.sync_to_drive()?;
        self.uptime_secs_col.sync_to_drive()?;
        self.temp_celsius_col.sync_to_drive()?;
        self.throttled_col.sync_to_drive()?;
        self.healthy_col.sync_to_drive()?;
        self.tombstones.sync_to_drive()?;
        self.tombstones.barrier()?;
        Ok(())
    }
    fn recover_from_wal(&mut self) {
        let __anchor = self.tombstones.len();
        while self.id_col.len() < __anchor {
            self.id_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.recorded_at_col.len() < __anchor {
            self.recorded_at_col
                .append_timestamp(0i64)
                .expect("Failed to backfill column");
        }
        while self.device_id_col.len() < __anchor {
            self.device_id_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.sample_seq_col.len() < __anchor {
            self.sample_seq_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.region_col.len() < __anchor {
            self.region_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.cpu_pct_col.len() < __anchor {
            self.cpu_pct_col.append_f64(0.0).expect("Failed to backfill column");
        }
        while self.mem_pct_col.len() < __anchor {
            self.mem_pct_col.append_f64(0.0).expect("Failed to backfill column");
        }
        while self.disk_pct_col.len() < __anchor {
            self.disk_pct_col.append_f64(0.0).expect("Failed to backfill column");
        }
        while self.net_rx_bytes_col.len() < __anchor {
            self.net_rx_bytes_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.net_tx_bytes_col.len() < __anchor {
            self.net_tx_bytes_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.req_count_col.len() < __anchor {
            self.req_count_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.err_count_col.len() < __anchor {
            self.err_count_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.p50_micros_col.len() < __anchor {
            self.p50_micros_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.p95_micros_col.len() < __anchor {
            self.p95_micros_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.p99_micros_col.len() < __anchor {
            self.p99_micros_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.queue_depth_col.len() < __anchor {
            self.queue_depth_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.open_conns_col.len() < __anchor {
            self.open_conns_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.gc_pause_micros_col.len() < __anchor {
            self.gc_pause_micros_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.uptime_secs_col.len() < __anchor {
            self.uptime_secs_col.append_i64(0).expect("Failed to backfill column");
        }
        while self.temp_celsius_col.len() < __anchor {
            self.temp_celsius_col.append_f64(0.0).expect("Failed to backfill column");
        }
        while self.throttled_col.len() < __anchor {
            self.throttled_col.append_bool(false).expect("Failed to backfill column");
        }
        while self.healthy_col.len() < __anchor {
            self.healthy_col.append_bool(false).expect("Failed to backfill column");
        }
        self.id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.recorded_at_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.device_id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.sample_seq_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.region_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.cpu_pct_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.mem_pct_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.disk_pct_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.net_rx_bytes_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.net_tx_bytes_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.req_count_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.err_count_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.p50_micros_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.p95_micros_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.p99_micros_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.queue_depth_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.open_conns_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.gc_pause_micros_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.uptime_secs_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.temp_celsius_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.throttled_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.healthy_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.row_count = __anchor;
        let __entries = self
            .wal
            .replay(|_| -> std::io::Result<()> { Ok(()) })
            .expect("Failed to replay WAL on recovery");
        for __entry in &__entries {
            if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                if payload.len() < 9 {
                    continue;
                }
                let mut __ri = [0u8; 8];
                __ri.copy_from_slice(&payload[0..8]);
                let __row_index = u64::from_le_bytes(__ri) as usize;
                if __row_index < self.row_count {
                    continue;
                }
                let __deleted = payload[8] != 0;
                let record: Metric = match serde_json::from_slice(&payload[9..]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                self.id_col
                    .append_uuid(*record.id.as_bytes())
                    .expect("Failed to append to column");
                self.recorded_at_col
                    .append_timestamp(i64::from(record.recorded_at))
                    .expect("Failed to append to column");
                self.device_id_col
                    .append_u64(record.device_id)
                    .expect("Failed to append to column");
                self.sample_seq_col
                    .append_u64(record.sample_seq)
                    .expect("Failed to append to column");
                self.region_col
                    .append_u32(record.region)
                    .expect("Failed to append to column");
                self.cpu_pct_col
                    .append_f64(record.cpu_pct)
                    .expect("Failed to append to column");
                self.mem_pct_col
                    .append_f64(record.mem_pct)
                    .expect("Failed to append to column");
                self.disk_pct_col
                    .append_f64(record.disk_pct)
                    .expect("Failed to append to column");
                self.net_rx_bytes_col
                    .append_u64(record.net_rx_bytes)
                    .expect("Failed to append to column");
                self.net_tx_bytes_col
                    .append_u64(record.net_tx_bytes)
                    .expect("Failed to append to column");
                self.req_count_col
                    .append_u64(record.req_count)
                    .expect("Failed to append to column");
                self.err_count_col
                    .append_u64(record.err_count)
                    .expect("Failed to append to column");
                self.p50_micros_col
                    .append_u32(record.p50_micros)
                    .expect("Failed to append to column");
                self.p95_micros_col
                    .append_u32(record.p95_micros)
                    .expect("Failed to append to column");
                self.p99_micros_col
                    .append_u32(record.p99_micros)
                    .expect("Failed to append to column");
                self.queue_depth_col
                    .append_u32(record.queue_depth)
                    .expect("Failed to append to column");
                self.open_conns_col
                    .append_u32(record.open_conns)
                    .expect("Failed to append to column");
                self.gc_pause_micros_col
                    .append_u32(record.gc_pause_micros)
                    .expect("Failed to append to column");
                self.uptime_secs_col
                    .append_i64(record.uptime_secs)
                    .expect("Failed to append to column");
                self.temp_celsius_col
                    .append_f64(record.temp_celsius)
                    .expect("Failed to append to column");
                self.throttled_col
                    .append_bool(record.throttled)
                    .expect("Failed to append to column");
                self.healthy_col
                    .append_bool(record.healthy)
                    .expect("Failed to append to column");
                self.tombstones
                    .append(__deleted)
                    .expect("Failed to append tombstone on recovery");
                self.row_count += 1;
            }
        }
    }
    pub fn checkpoint(&mut self) {
        self.id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.recorded_at_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.device_id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.sample_seq_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.region_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.cpu_pct_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.mem_pct_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.disk_pct_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.net_rx_bytes_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.net_tx_bytes_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.req_count_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.err_count_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.p50_micros_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.p95_micros_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.p99_micros_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.queue_depth_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.open_conns_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.gc_pause_micros_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.uptime_secs_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.temp_celsius_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.throttled_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.healthy_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync tombstones to drive on checkpoint");
        self.tombstones.barrier().expect("Failed to issue checkpoint device barrier");
        self.wal.truncate().expect("Failed to truncate WAL on checkpoint");
        self.writes_since_checkpoint = 0;
    }
    pub fn compact(&mut self) {
        if self.in_transaction {
            self.compact_deferred = true;
            return;
        }
        self.checkpoint();
        let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
        for &__row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                __keep.push(__row);
            }
        }
        let __config = forgedb_compaction::CompactionConfig::default();
        let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
        if __compactor.compact_model_keeping("metric", &__keep).is_ok() {
            let mut __keep_sorted = __keep;
            __keep_sorted.sort_unstable();
            let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
            let __saved_device_id_index = std::sync::Arc::clone(&self.device_id_index);
            let __saved_device_id_ordered = std::sync::Arc::clone(
                &self.device_id_ordered,
            );
            let __root = self.root.clone();
            let __feed = self.changefeed.take();
            let __broker = self.broker.take();
            *self = Self::new_at_no_rehydrate(&__root);
            self.changefeed = __feed;
            self.broker = __broker;
            self.device_id_index = __saved_device_id_index;
            self.device_id_ordered = __saved_device_id_ordered;
            let mut __new_id_to_row: std::collections::HashMap<_, usize> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            for (&__id, &__old_row) in __old_id_to_row.iter() {
                if __keep_sorted.binary_search(&__old_row).is_ok() {
                    let __new_row = __keep_sorted
                        .partition_point(|&__r| __r < __old_row);
                    __new_id_to_row.insert(__id, __new_row);
                    __new_id_versions.insert(__id, vec![__new_row]);
                }
            }
            self.id_to_row = std::sync::Arc::new(__new_id_to_row);
            self.id_versions = std::sync::Arc::new(__new_id_versions);
            self.bump_compaction_epoch(&__root);
        }
        self.dead_since_compaction = 0;
        self.compaction_due = false;
    }
    pub fn maintain(&mut self) {
        if self.compaction_due && !self.in_transaction {
            self.compaction_due = false;
            self.compact();
        }
    }
    pub fn __reindex_committed(&mut self) {
        std::sync::Arc::make_mut(&mut self.id_to_row).clear();
        std::sync::Arc::make_mut(&mut self.id_versions).clear();
        std::sync::Arc::make_mut(&mut self.device_id_index).clear();
        let n = self.tombstones.len();
        self.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = self.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(i);
        }
        let __ids: Vec<Uuid> = self.id_to_row.keys().copied().collect();
        for __id in __ids {
            let __row = match self.id_to_row.get(&__id) {
                Some(__r) => *__r,
                None => continue,
            };
            if self.tombstones.is_deleted(__row).unwrap_or(true) {
                continue;
            }
            let device_id_value = self
                .device_id_col
                .read_u64(__row)
                .expect("Failed to read from column");
            {
                let __k: String = {
                    {
                        let __v = &(device_id_value);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                std::sync::Arc::make_mut(&mut self.device_id_index)
                    .entry(__k)
                    .or_default()
                    .insert(__id);
            }
            {
                std::sync::Arc::make_mut(&mut self.device_id_ordered)
                    .entry(device_id_value)
                    .or_default()
                    .insert(__id);
            }
        }
    }
    pub fn __reindex_delta(&mut self, from: usize) {
        let __n = self.row_count;
        for __r in from..__n {
            let id = {
                let bytes = self
                    .id_col
                    .read_uuid(__r)
                    .expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            if let Some(__old_rec) = self.get(id) {
                {
                    let __k: String = {
                        {
                            let __v = &(__old_rec.device_id);
                            use std::fmt::Write as _;
                            let mut __k = String::from('\u{2}');
                            let _ = write!(__k, "{}", __v);
                            __k
                        }
                    };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.device_id_index);
                    if let Some(__set) = __map.get_mut(&__k) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__k);
                    }
                }
                {
                    let __ok = { __old_rec.device_id };
                    let mut __empty = false;
                    let __map = std::sync::Arc::make_mut(&mut self.device_id_ordered);
                    if let Some(__set) = __map.get_mut(&__ok) {
                        __set.remove(&(id));
                        __empty = __set.is_empty();
                    }
                    if __empty {
                        __map.remove(&__ok);
                    }
                }
            }
            let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(__r);
            if !__deleted {
                if let Some(__new_rec) = self.read_at(__r) {
                    {
                        let __k: String = {
                            {
                                let __v = &(__new_rec.device_id);
                                use std::fmt::Write as _;
                                let mut __k = String::from('\u{2}');
                                let _ = write!(__k, "{}", __v);
                                __k
                            }
                        };
                        std::sync::Arc::make_mut(&mut self.device_id_index)
                            .entry(__k)
                            .or_default()
                            .insert(id);
                    }
                    {
                        std::sync::Arc::make_mut(&mut self.device_id_ordered)
                            .entry(__new_rec.device_id)
                            .or_default()
                            .insert(id);
                    }
                }
            }
        }
    }
    pub fn __sync_columns_from_disk(&mut self) {
        let _ = self.id_col.sync_from_disk();
        let _ = self.recorded_at_col.sync_from_disk();
        let _ = self.device_id_col.sync_from_disk();
        let _ = self.sample_seq_col.sync_from_disk();
        let _ = self.region_col.sync_from_disk();
        let _ = self.cpu_pct_col.sync_from_disk();
        let _ = self.mem_pct_col.sync_from_disk();
        let _ = self.disk_pct_col.sync_from_disk();
        let _ = self.net_rx_bytes_col.sync_from_disk();
        let _ = self.net_tx_bytes_col.sync_from_disk();
        let _ = self.req_count_col.sync_from_disk();
        let _ = self.err_count_col.sync_from_disk();
        let _ = self.p50_micros_col.sync_from_disk();
        let _ = self.p95_micros_col.sync_from_disk();
        let _ = self.p99_micros_col.sync_from_disk();
        let _ = self.queue_depth_col.sync_from_disk();
        let _ = self.open_conns_col.sync_from_disk();
        let _ = self.gc_pause_micros_col.sync_from_disk();
        let _ = self.uptime_secs_col.sync_from_disk();
        let _ = self.temp_celsius_col.sync_from_disk();
        let _ = self.throttled_col.sync_from_disk();
        let _ = self.healthy_col.sync_from_disk();
        let _ = self.tombstones.sync_from_disk();
        self.row_count = self.tombstones.len();
    }
    pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
        self.id_col.truncate_to_rows(rows)?;
        self.recorded_at_col.truncate_to_rows(rows)?;
        self.device_id_col.truncate_to_rows(rows)?;
        self.sample_seq_col.truncate_to_rows(rows)?;
        self.region_col.truncate_to_rows(rows)?;
        self.cpu_pct_col.truncate_to_rows(rows)?;
        self.mem_pct_col.truncate_to_rows(rows)?;
        self.disk_pct_col.truncate_to_rows(rows)?;
        self.net_rx_bytes_col.truncate_to_rows(rows)?;
        self.net_tx_bytes_col.truncate_to_rows(rows)?;
        self.req_count_col.truncate_to_rows(rows)?;
        self.err_count_col.truncate_to_rows(rows)?;
        self.p50_micros_col.truncate_to_rows(rows)?;
        self.p95_micros_col.truncate_to_rows(rows)?;
        self.p99_micros_col.truncate_to_rows(rows)?;
        self.queue_depth_col.truncate_to_rows(rows)?;
        self.open_conns_col.truncate_to_rows(rows)?;
        self.gc_pause_micros_col.truncate_to_rows(rows)?;
        self.uptime_secs_col.truncate_to_rows(rows)?;
        self.temp_celsius_col.truncate_to_rows(rows)?;
        self.throttled_col.truncate_to_rows(rows)?;
        self.healthy_col.truncate_to_rows(rows)?;
        self.tombstones.truncate_to_rows(rows)?;
        Ok(())
    }
    pub fn __recover_to_committed(&mut self, committed_len: usize) {
        if committed_len >= self.row_count {
            return;
        }
        self.id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.recorded_at_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.device_id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.sample_seq_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.region_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.cpu_pct_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.mem_pct_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.disk_pct_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.net_rx_bytes_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.net_tx_bytes_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.req_count_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.err_count_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.p50_micros_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.p95_micros_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.p99_micros_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.queue_depth_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.open_conns_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.gc_pause_micros_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.uptime_secs_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.temp_celsius_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.throttled_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.healthy_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.tombstones
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate tombstones on journal recovery");
        self.row_count = committed_len;
        self.wal.truncate().expect("Failed to truncate WAL on journal recovery");
        let __root = self.root.clone();
        let __feed = self.changefeed.take();
        let __broker = self.broker.take();
        *self = Self::new_at(&__root);
        self.changefeed = __feed;
        self.broker = __broker;
    }
    pub fn __stage_append(&mut self, record: Metric, deleted: bool) -> usize {
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        if deleted {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(1u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Metric", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        } else {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(0u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Metric", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.recorded_at_col
            .append_timestamp(i64::from(record.recorded_at))
            .expect("Failed to append to column");
        self.device_id_col
            .append_u64(record.device_id)
            .expect("Failed to append to column");
        self.sample_seq_col
            .append_u64(record.sample_seq)
            .expect("Failed to append to column");
        self.region_col.append_u32(record.region).expect("Failed to append to column");
        self.cpu_pct_col.append_f64(record.cpu_pct).expect("Failed to append to column");
        self.mem_pct_col.append_f64(record.mem_pct).expect("Failed to append to column");
        self.disk_pct_col
            .append_f64(record.disk_pct)
            .expect("Failed to append to column");
        self.net_rx_bytes_col
            .append_u64(record.net_rx_bytes)
            .expect("Failed to append to column");
        self.net_tx_bytes_col
            .append_u64(record.net_tx_bytes)
            .expect("Failed to append to column");
        self.req_count_col
            .append_u64(record.req_count)
            .expect("Failed to append to column");
        self.err_count_col
            .append_u64(record.err_count)
            .expect("Failed to append to column");
        self.p50_micros_col
            .append_u32(record.p50_micros)
            .expect("Failed to append to column");
        self.p95_micros_col
            .append_u32(record.p95_micros)
            .expect("Failed to append to column");
        self.p99_micros_col
            .append_u32(record.p99_micros)
            .expect("Failed to append to column");
        self.queue_depth_col
            .append_u32(record.queue_depth)
            .expect("Failed to append to column");
        self.open_conns_col
            .append_u32(record.open_conns)
            .expect("Failed to append to column");
        self.gc_pause_micros_col
            .append_u32(record.gc_pause_micros)
            .expect("Failed to append to column");
        self.uptime_secs_col
            .append_i64(record.uptime_secs)
            .expect("Failed to append to column");
        self.temp_celsius_col
            .append_f64(record.temp_celsius)
            .expect("Failed to append to column");
        self.throttled_col
            .append_bool(record.throttled)
            .expect("Failed to append to column");
        self.healthy_col
            .append_bool(record.healthy)
            .expect("Failed to append to column");
        self.tombstones.append(deleted).expect("Failed to append tombstone");
        self.row_count += 1;
        row_index
    }
    pub fn run_deferred_maintenance(&mut self) {
        if self.compact_deferred {
            self.compact_deferred = false;
            self.checkpoint_deferred = false;
            self.compact();
        } else if self.checkpoint_deferred {
            self.checkpoint_deferred = false;
            self.checkpoint();
        }
    }
    pub fn read_at(&self, row_index: usize) -> Option<Metric> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let recorded_at_value = Timestamp::from(
            self
                .recorded_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        let device_id_value = self
            .device_id_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let sample_seq_value = self
            .sample_seq_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let region_value = self
            .region_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let cpu_pct_value = self
            .cpu_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let mem_pct_value = self
            .mem_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let disk_pct_value = self
            .disk_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let net_rx_bytes_value = self
            .net_rx_bytes_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let net_tx_bytes_value = self
            .net_tx_bytes_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let req_count_value = self
            .req_count_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let err_count_value = self
            .err_count_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let p50_micros_value = self
            .p50_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let p95_micros_value = self
            .p95_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let p99_micros_value = self
            .p99_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let queue_depth_value = self
            .queue_depth_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let open_conns_value = self
            .open_conns_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let gc_pause_micros_value = self
            .gc_pause_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let uptime_secs_value = self
            .uptime_secs_col
            .read_i64(row_index)
            .expect("Failed to read from column");
        let temp_celsius_value = self
            .temp_celsius_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let throttled_value = self
            .throttled_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        let healthy_value = self
            .healthy_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        Some(Metric {
            id: id_value,
            recorded_at: recorded_at_value,
            device_id: device_id_value,
            sample_seq: sample_seq_value,
            region: region_value,
            cpu_pct: cpu_pct_value,
            mem_pct: mem_pct_value,
            disk_pct: disk_pct_value,
            net_rx_bytes: net_rx_bytes_value,
            net_tx_bytes: net_tx_bytes_value,
            req_count: req_count_value,
            err_count: err_count_value,
            p50_micros: p50_micros_value,
            p95_micros: p95_micros_value,
            p99_micros: p99_micros_value,
            queue_depth: queue_depth_value,
            open_conns: open_conns_value,
            gc_pause_micros: gc_pause_micros_value,
            uptime_secs: uptime_secs_value,
            temp_celsius: temp_celsius_value,
            throttled: throttled_value,
            healthy: healthy_value,
        })
    }
    pub fn get(&self, id: Uuid) -> Option<Metric> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_at(row_index)
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Metric> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Metric> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn all(&self) -> Vec<Metric> {
        let mut records = Vec::new();
        for id in self.id_to_row.keys() {
            if let Some(record) = self.get(*id) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_device_id(&self, value: u64) -> Vec<Metric> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.device_id_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        __ids.iter().filter_map(|&__id| self.get(__id)).collect()
    }
    pub fn find_by_device_id_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: u64,
    ) -> Vec<Metric> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.device_id_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.device_id);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
    pub fn find_by_device_id_range(
        &self,
        min: Option<u64>,
        max: Option<u64>,
        descending: bool,
        limit: Option<usize>,
    ) -> Vec<Metric> {
        let __lo_v: Option<u64> = min.map(|__v| __v);
        let __hi_v: Option<u64> = max.map(|__v| __v);
        if let (Some(__l), Some(__h)) = (__lo_v.as_ref(), __hi_v.as_ref()) {
            if __l > __h {
                return Vec::new();
            }
        }
        let __lo = match __lo_v {
            Some(__v) => std::ops::Bound::Included(__v),
            None => std::ops::Bound::Unbounded,
        };
        let __hi = match __hi_v {
            Some(__v) => std::ops::Bound::Included(__v),
            None => std::ops::Bound::Unbounded,
        };
        let mut __out: Vec<Metric> = Vec::new();
        let __iter: Box<
            dyn Iterator<Item = (&u64, &std::collections::BTreeSet<Uuid>)> + '_,
        > = if descending {
            Box::new(self.device_id_ordered.range((__lo, __hi)).rev())
        } else {
            Box::new(self.device_id_ordered.range((__lo, __hi)))
        };
        'outer: for (_k, __ids) in __iter {
            for &__id in __ids {
                if let Some(__r) = self.get(__id) {
                    __out.push(__r);
                    if let Some(__n) = limit {
                        if __out.len() >= __n {
                            break 'outer;
                        }
                    }
                }
            }
        }
        __out
    }
    pub fn export_live_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        for &row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(row).unwrap_or(true) {
                indices.push(row);
            }
        }
        indices.sort_unstable();
        indices
    }
    pub fn export_col_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.id_col.export(indices)
    }
    pub fn export_col_recorded_at(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.recorded_at_col.export(indices)
    }
    pub fn export_col_device_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.device_id_col.export(indices)
    }
    pub fn export_col_sample_seq(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.sample_seq_col.export(indices)
    }
    pub fn export_col_region(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.region_col.export(indices)
    }
    pub fn export_col_cpu_pct(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.cpu_pct_col.export(indices)
    }
    pub fn export_col_mem_pct(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.mem_pct_col.export(indices)
    }
    pub fn export_col_disk_pct(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.disk_pct_col.export(indices)
    }
    pub fn export_col_net_rx_bytes(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.net_rx_bytes_col.export(indices)
    }
    pub fn export_col_net_tx_bytes(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.net_tx_bytes_col.export(indices)
    }
    pub fn export_col_req_count(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.req_count_col.export(indices)
    }
    pub fn export_col_err_count(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.err_count_col.export(indices)
    }
    pub fn export_col_p50_micros(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.p50_micros_col.export(indices)
    }
    pub fn export_col_p95_micros(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.p95_micros_col.export(indices)
    }
    pub fn export_col_p99_micros(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.p99_micros_col.export(indices)
    }
    pub fn export_col_queue_depth(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.queue_depth_col.export(indices)
    }
    pub fn export_col_open_conns(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.open_conns_col.export(indices)
    }
    pub fn export_col_gc_pause_micros(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.gc_pause_micros_col.export(indices)
    }
    pub fn export_col_uptime_secs(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.uptime_secs_col.export(indices)
    }
    pub fn export_col_temp_celsius(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.temp_celsius_col.export(indices)
    }
    fn __proj_resolve_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<usize> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        Some(versions[pos - 1])
    }
    fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
        let watermark = snap.watermark();
        let mut rows = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            rows.push(versions[pos - 1]);
        }
        rows
    }
    pub fn read_hot_at(&self, row_index: usize) -> Option<MetricHot> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let cpu_pct_value = self
            .cpu_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let mem_pct_value = self
            .mem_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        Some(MetricHot {
            id: id_value,
            cpu_pct: cpu_pct_value,
            mem_pct: mem_pct_value,
        })
    }
    pub fn get_hot(&self, id: Uuid) -> Option<MetricHot> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_hot_at(row_index)
    }
    pub fn all_hot(&self) -> Vec<MetricHot> {
        let mut __rows: Vec<usize> = self.id_to_row.values().copied().collect();
        __rows.sort_unstable();
        let __rows = self
            .tombstones
            .live_indices(&__rows)
            .expect("Failed to read tombstone liveness");
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __MetricHotScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            cpu_pct_col: forgedb_storage::BufferedFixedColumn,
            mem_pct_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __MetricHotScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            cpu_pct_col: self
                .cpu_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            mem_pct_col: self
                .mem_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut rows = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let cpu_pct_value = __bufs
                .cpu_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let mem_pct_value = __bufs
                .mem_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            rows.push(MetricHot {
                id: id_value,
                cpu_pct: cpu_pct_value,
                mem_pct: mem_pct_value,
            });
        }
        rows
    }
    pub fn get_hot_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<MetricHot> {
        let row_index = self.__proj_resolve_at(snap, id)?;
        self.read_hot_at(row_index)
    }
    pub fn all_hot_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<MetricHot> {
        let mut records = Vec::new();
        for row_index in self.__proj_live_rows_at(snap) {
            if let Some(record) = self.read_hot_at(row_index) {
                records.push(record);
            }
        }
        records
    }
    pub fn __with_scan<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&MetricScanRef<'_>) -> bool,
        f: impl FnOnce(&mut Vec<MetricScanRef<'_>>) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __MetricScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            recorded_at_col: forgedb_storage::BufferedFixedColumn,
            device_id_col: forgedb_storage::BufferedFixedColumn,
            sample_seq_col: forgedb_storage::BufferedFixedColumn,
            region_col: forgedb_storage::BufferedFixedColumn,
            cpu_pct_col: forgedb_storage::BufferedFixedColumn,
            mem_pct_col: forgedb_storage::BufferedFixedColumn,
            disk_pct_col: forgedb_storage::BufferedFixedColumn,
            net_rx_bytes_col: forgedb_storage::BufferedFixedColumn,
            net_tx_bytes_col: forgedb_storage::BufferedFixedColumn,
            req_count_col: forgedb_storage::BufferedFixedColumn,
            err_count_col: forgedb_storage::BufferedFixedColumn,
            p50_micros_col: forgedb_storage::BufferedFixedColumn,
            p95_micros_col: forgedb_storage::BufferedFixedColumn,
            p99_micros_col: forgedb_storage::BufferedFixedColumn,
            queue_depth_col: forgedb_storage::BufferedFixedColumn,
            open_conns_col: forgedb_storage::BufferedFixedColumn,
            gc_pause_micros_col: forgedb_storage::BufferedFixedColumn,
            uptime_secs_col: forgedb_storage::BufferedFixedColumn,
            temp_celsius_col: forgedb_storage::BufferedFixedColumn,
            throttled_col: forgedb_storage::BufferedFixedColumn,
            healthy_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __MetricScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            recorded_at_col: self
                .recorded_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            device_id_col: self
                .device_id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            sample_seq_col: self
                .sample_seq_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            region_col: self
                .region_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            cpu_pct_col: self
                .cpu_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            mem_pct_col: self
                .mem_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            disk_pct_col: self
                .disk_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            net_rx_bytes_col: self
                .net_rx_bytes_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            net_tx_bytes_col: self
                .net_tx_bytes_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            req_count_col: self
                .req_count_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            err_count_col: self
                .err_count_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p50_micros_col: self
                .p50_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p95_micros_col: self
                .p95_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p99_micros_col: self
                .p99_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            queue_depth_col: self
                .queue_depth_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            open_conns_col: self
                .open_conns_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            gc_pause_micros_col: self
                .gc_pause_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            uptime_secs_col: self
                .uptime_secs_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            temp_celsius_col: self
                .temp_celsius_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            throttled_col: self
                .throttled_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            healthy_col: self
                .healthy_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<MetricScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let recorded_at_value = Timestamp::from(
                __bufs
                    .recorded_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let device_id_value = __bufs
                .device_id_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let sample_seq_value = __bufs
                .sample_seq_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let region_value = __bufs
                .region_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let cpu_pct_value = __bufs
                .cpu_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let mem_pct_value = __bufs
                .mem_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let disk_pct_value = __bufs
                .disk_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let net_rx_bytes_value = __bufs
                .net_rx_bytes_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let net_tx_bytes_value = __bufs
                .net_tx_bytes_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let req_count_value = __bufs
                .req_count_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let err_count_value = __bufs
                .err_count_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let p50_micros_value = __bufs
                .p50_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let p95_micros_value = __bufs
                .p95_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let p99_micros_value = __bufs
                .p99_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let queue_depth_value = __bufs
                .queue_depth_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let open_conns_value = __bufs
                .open_conns_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let gc_pause_micros_value = __bufs
                .gc_pause_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let uptime_secs_value = __bufs
                .uptime_secs_col
                .read_i64(__slot)
                .expect("Failed to read from column");
            let temp_celsius_value = __bufs
                .temp_celsius_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let throttled_value = __bufs
                .throttled_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let healthy_value = __bufs
                .healthy_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let __row_ref = MetricScanRef {
                __slot,
                id: id_value,
                recorded_at: recorded_at_value,
                device_id: device_id_value,
                sample_seq: sample_seq_value,
                region: region_value,
                cpu_pct: cpu_pct_value,
                mem_pct: mem_pct_value,
                disk_pct: disk_pct_value,
                net_rx_bytes: net_rx_bytes_value,
                net_tx_bytes: net_tx_bytes_value,
                req_count: req_count_value,
                err_count: err_count_value,
                p50_micros: p50_micros_value,
                p95_micros: p95_micros_value,
                p99_micros: p99_micros_value,
                queue_depth: queue_depth_value,
                open_conns: open_conns_value,
                gc_pause_micros: gc_pause_micros_value,
                uptime_secs: uptime_secs_value,
                temp_celsius: temp_celsius_value,
                throttled: throttled_value,
                healthy: healthy_value,
                __borrow: ::std::marker::PhantomData,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        f(&mut __refs)
    }
    pub fn __with_page<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&MetricScanRef<'_>) -> bool,
        sort: impl FnOnce(&mut Vec<MetricScanRef<'_>>),
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[MetricPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __MetricPageScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            recorded_at_col: forgedb_storage::BufferedFixedColumn,
            device_id_col: forgedb_storage::BufferedFixedColumn,
            sample_seq_col: forgedb_storage::BufferedFixedColumn,
            region_col: forgedb_storage::BufferedFixedColumn,
            cpu_pct_col: forgedb_storage::BufferedFixedColumn,
            mem_pct_col: forgedb_storage::BufferedFixedColumn,
            disk_pct_col: forgedb_storage::BufferedFixedColumn,
            net_rx_bytes_col: forgedb_storage::BufferedFixedColumn,
            net_tx_bytes_col: forgedb_storage::BufferedFixedColumn,
            req_count_col: forgedb_storage::BufferedFixedColumn,
            err_count_col: forgedb_storage::BufferedFixedColumn,
            p50_micros_col: forgedb_storage::BufferedFixedColumn,
            p95_micros_col: forgedb_storage::BufferedFixedColumn,
            p99_micros_col: forgedb_storage::BufferedFixedColumn,
            queue_depth_col: forgedb_storage::BufferedFixedColumn,
            open_conns_col: forgedb_storage::BufferedFixedColumn,
            gc_pause_micros_col: forgedb_storage::BufferedFixedColumn,
            uptime_secs_col: forgedb_storage::BufferedFixedColumn,
            temp_celsius_col: forgedb_storage::BufferedFixedColumn,
            throttled_col: forgedb_storage::BufferedFixedColumn,
            healthy_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __MetricPageScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            recorded_at_col: self
                .recorded_at_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            device_id_col: self
                .device_id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            sample_seq_col: self
                .sample_seq_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            region_col: self
                .region_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            cpu_pct_col: self
                .cpu_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            mem_pct_col: self
                .mem_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            disk_pct_col: self
                .disk_pct_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            net_rx_bytes_col: self
                .net_rx_bytes_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            net_tx_bytes_col: self
                .net_tx_bytes_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            req_count_col: self
                .req_count_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            err_count_col: self
                .err_count_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p50_micros_col: self
                .p50_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p95_micros_col: self
                .p95_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            p99_micros_col: self
                .p99_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            queue_depth_col: self
                .queue_depth_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            open_conns_col: self
                .open_conns_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            gc_pause_micros_col: self
                .gc_pause_micros_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            uptime_secs_col: self
                .uptime_secs_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            temp_celsius_col: self
                .temp_celsius_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            throttled_col: self
                .throttled_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            healthy_col: self
                .healthy_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<MetricScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let recorded_at_value = Timestamp::from(
                __bufs
                    .recorded_at_col
                    .read_timestamp(__slot)
                    .expect("Failed to read from column"),
            );
            let device_id_value = __bufs
                .device_id_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let sample_seq_value = __bufs
                .sample_seq_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let region_value = __bufs
                .region_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let cpu_pct_value = __bufs
                .cpu_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let mem_pct_value = __bufs
                .mem_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let disk_pct_value = __bufs
                .disk_pct_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let net_rx_bytes_value = __bufs
                .net_rx_bytes_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let net_tx_bytes_value = __bufs
                .net_tx_bytes_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let req_count_value = __bufs
                .req_count_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let err_count_value = __bufs
                .err_count_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let p50_micros_value = __bufs
                .p50_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let p95_micros_value = __bufs
                .p95_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let p99_micros_value = __bufs
                .p99_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let queue_depth_value = __bufs
                .queue_depth_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let open_conns_value = __bufs
                .open_conns_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let gc_pause_micros_value = __bufs
                .gc_pause_micros_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let uptime_secs_value = __bufs
                .uptime_secs_col
                .read_i64(__slot)
                .expect("Failed to read from column");
            let temp_celsius_value = __bufs
                .temp_celsius_col
                .read_f64(__slot)
                .expect("Failed to read from column");
            let throttled_value = __bufs
                .throttled_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let healthy_value = __bufs
                .healthy_col
                .read_bool(__slot)
                .expect("Failed to read from column");
            let __row_ref = MetricScanRef {
                __slot,
                id: id_value,
                recorded_at: recorded_at_value,
                device_id: device_id_value,
                sample_seq: sample_seq_value,
                region: region_value,
                cpu_pct: cpu_pct_value,
                mem_pct: mem_pct_value,
                disk_pct: disk_pct_value,
                net_rx_bytes: net_rx_bytes_value,
                net_tx_bytes: net_tx_bytes_value,
                req_count: req_count_value,
                err_count: err_count_value,
                p50_micros: p50_micros_value,
                p95_micros: p95_micros_value,
                p99_micros: p99_micros_value,
                queue_depth: queue_depth_value,
                open_conns: open_conns_value,
                gc_pause_micros: gc_pause_micros_value,
                uptime_secs: uptime_secs_value,
                temp_celsius: temp_celsius_value,
                throttled: throttled_value,
                healthy: healthy_value,
                __borrow: ::std::marker::PhantomData,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        let __total = __refs.len();
        sort(&mut __refs);
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        let __page = &__refs[__start..__end];
        let __page_rows: Vec<usize> = __page
            .iter()
            .map(|__r| __rows[__r.__slot])
            .collect();
        #[allow(non_camel_case_types)]
        struct __MetricPageBufs {}
        let __page_bufs = __MetricPageBufs {};
        let mut __views: Vec<MetricPageRef<'_>> = Vec::with_capacity(__page.len());
        for (__pslot, __ref) in __page.iter().enumerate() {
            __views
                .push(MetricPageRef {
                    id: __ref.id,
                    recorded_at: __ref.recorded_at,
                    device_id: __ref.device_id,
                    sample_seq: __ref.sample_seq,
                    region: __ref.region,
                    cpu_pct: __ref.cpu_pct,
                    mem_pct: __ref.mem_pct,
                    disk_pct: __ref.disk_pct,
                    net_rx_bytes: __ref.net_rx_bytes,
                    net_tx_bytes: __ref.net_tx_bytes,
                    req_count: __ref.req_count,
                    err_count: __ref.err_count,
                    p50_micros: __ref.p50_micros,
                    p95_micros: __ref.p95_micros,
                    p99_micros: __ref.p99_micros,
                    queue_depth: __ref.queue_depth,
                    open_conns: __ref.open_conns,
                    gc_pause_micros: __ref.gc_pause_micros,
                    uptime_secs: __ref.uptime_secs,
                    temp_celsius: __ref.temp_celsius,
                    throttled: __ref.throttled,
                    healthy: __ref.healthy,
                    __borrow: ::std::marker::PhantomData,
                });
        }
        f(__total, &__views)
    }
    pub fn __with_fast_page<R>(
        &self,
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[MetricPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = {
            let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
            __all.sort_unstable();
            self.tombstones
                .live_indices(&__all)
                .expect("Failed to read tombstone liveness")
        };
        let __total = __rows.len();
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        #[allow(non_camel_case_types)]
        struct __MetricFastPageBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            recorded_at_col: forgedb_storage::BufferedFixedColumn,
            device_id_col: forgedb_storage::BufferedFixedColumn,
            sample_seq_col: forgedb_storage::BufferedFixedColumn,
            region_col: forgedb_storage::BufferedFixedColumn,
            cpu_pct_col: forgedb_storage::BufferedFixedColumn,
            mem_pct_col: forgedb_storage::BufferedFixedColumn,
            disk_pct_col: forgedb_storage::BufferedFixedColumn,
            net_rx_bytes_col: forgedb_storage::BufferedFixedColumn,
            net_tx_bytes_col: forgedb_storage::BufferedFixedColumn,
            req_count_col: forgedb_storage::BufferedFixedColumn,
            err_count_col: forgedb_storage::BufferedFixedColumn,
            p50_micros_col: forgedb_storage::BufferedFixedColumn,
            p95_micros_col: forgedb_storage::BufferedFixedColumn,
            p99_micros_col: forgedb_storage::BufferedFixedColumn,
            queue_depth_col: forgedb_storage::BufferedFixedColumn,
            open_conns_col: forgedb_storage::BufferedFixedColumn,
            gc_pause_micros_col: forgedb_storage::BufferedFixedColumn,
            uptime_secs_col: forgedb_storage::BufferedFixedColumn,
            temp_celsius_col: forgedb_storage::BufferedFixedColumn,
            throttled_col: forgedb_storage::BufferedFixedColumn,
            healthy_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __MetricFastPageBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            recorded_at_col: self
                .recorded_at_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            device_id_col: self
                .device_id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            sample_seq_col: self
                .sample_seq_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            region_col: self
                .region_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            cpu_pct_col: self
                .cpu_pct_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            mem_pct_col: self
                .mem_pct_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            disk_pct_col: self
                .disk_pct_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            net_rx_bytes_col: self
                .net_rx_bytes_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            net_tx_bytes_col: self
                .net_tx_bytes_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            req_count_col: self
                .req_count_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            err_count_col: self
                .err_count_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            p50_micros_col: self
                .p50_micros_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            p95_micros_col: self
                .p95_micros_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            p99_micros_col: self
                .p99_micros_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            queue_depth_col: self
                .queue_depth_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            open_conns_col: self
                .open_conns_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            gc_pause_micros_col: self
                .gc_pause_micros_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            uptime_secs_col: self
                .uptime_secs_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            temp_celsius_col: self
                .temp_celsius_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            throttled_col: self
                .throttled_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            healthy_col: self
                .healthy_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
        };
        let mut __views: Vec<MetricPageRef<'_>> = Vec::with_capacity(__end - __start);
        for __pslot in 0..(__end - __start) {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let recorded_at_value = Timestamp::from(
                __bufs
                    .recorded_at_col
                    .read_timestamp(__pslot)
                    .expect("Failed to read from column"),
            );
            let device_id_value = __bufs
                .device_id_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let sample_seq_value = __bufs
                .sample_seq_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let region_value = __bufs
                .region_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let cpu_pct_value = __bufs
                .cpu_pct_col
                .read_f64(__pslot)
                .expect("Failed to read from column");
            let mem_pct_value = __bufs
                .mem_pct_col
                .read_f64(__pslot)
                .expect("Failed to read from column");
            let disk_pct_value = __bufs
                .disk_pct_col
                .read_f64(__pslot)
                .expect("Failed to read from column");
            let net_rx_bytes_value = __bufs
                .net_rx_bytes_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let net_tx_bytes_value = __bufs
                .net_tx_bytes_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let req_count_value = __bufs
                .req_count_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let err_count_value = __bufs
                .err_count_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let p50_micros_value = __bufs
                .p50_micros_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let p95_micros_value = __bufs
                .p95_micros_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let p99_micros_value = __bufs
                .p99_micros_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let queue_depth_value = __bufs
                .queue_depth_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let open_conns_value = __bufs
                .open_conns_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let gc_pause_micros_value = __bufs
                .gc_pause_micros_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let uptime_secs_value = __bufs
                .uptime_secs_col
                .read_i64(__pslot)
                .expect("Failed to read from column");
            let temp_celsius_value = __bufs
                .temp_celsius_col
                .read_f64(__pslot)
                .expect("Failed to read from column");
            let throttled_value = __bufs
                .throttled_col
                .read_bool(__pslot)
                .expect("Failed to read from column");
            let healthy_value = __bufs
                .healthy_col
                .read_bool(__pslot)
                .expect("Failed to read from column");
            __views
                .push(MetricPageRef {
                    id: id_value,
                    recorded_at: recorded_at_value,
                    device_id: device_id_value,
                    sample_seq: sample_seq_value,
                    region: region_value,
                    cpu_pct: cpu_pct_value,
                    mem_pct: mem_pct_value,
                    disk_pct: disk_pct_value,
                    net_rx_bytes: net_rx_bytes_value,
                    net_tx_bytes: net_tx_bytes_value,
                    req_count: req_count_value,
                    err_count: err_count_value,
                    p50_micros: p50_micros_value,
                    p95_micros: p95_micros_value,
                    p99_micros: p99_micros_value,
                    queue_depth: queue_depth_value,
                    open_conns: open_conns_value,
                    gc_pause_micros: gc_pause_micros_value,
                    uptime_secs: uptime_secs_value,
                    temp_celsius: temp_celsius_value,
                    throttled: throttled_value,
                    healthy: healthy_value,
                    __borrow: ::std::marker::PhantomData,
                });
        }
        f(__total, &__views)
    }
    pub fn __rows_by_device_id(&self, value: &str) -> Option<Vec<usize>> {
        let __typed = value.parse::<u64>().ok()?;
        let __k: String = {
            {
                let __v = &(__typed);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.device_id_index.get(&__k) {
            Some(__s) => __s,
            None => return Some(Vec::new()),
        };
        let mut __out = Vec::with_capacity(__ids.len());
        for &__id in __ids {
            if let Some(&__row) = self.id_to_row.get(&__id) {
                __out.push(__row);
            }
        }
        Some(__out)
    }
    pub fn reader(&self) -> MetricStorageReader {
        MetricStorageReader {
            id_to_row: self.id_to_row.clone(),
            id_versions: self.id_versions.clone(),
            id_col: self.id_col.reader().expect("Failed to open column reader"),
            recorded_at_col: self
                .recorded_at_col
                .reader()
                .expect("Failed to open column reader"),
            device_id_col: self
                .device_id_col
                .reader()
                .expect("Failed to open column reader"),
            sample_seq_col: self
                .sample_seq_col
                .reader()
                .expect("Failed to open column reader"),
            region_col: self.region_col.reader().expect("Failed to open column reader"),
            cpu_pct_col: self
                .cpu_pct_col
                .reader()
                .expect("Failed to open column reader"),
            mem_pct_col: self
                .mem_pct_col
                .reader()
                .expect("Failed to open column reader"),
            disk_pct_col: self
                .disk_pct_col
                .reader()
                .expect("Failed to open column reader"),
            net_rx_bytes_col: self
                .net_rx_bytes_col
                .reader()
                .expect("Failed to open column reader"),
            net_tx_bytes_col: self
                .net_tx_bytes_col
                .reader()
                .expect("Failed to open column reader"),
            req_count_col: self
                .req_count_col
                .reader()
                .expect("Failed to open column reader"),
            err_count_col: self
                .err_count_col
                .reader()
                .expect("Failed to open column reader"),
            p50_micros_col: self
                .p50_micros_col
                .reader()
                .expect("Failed to open column reader"),
            p95_micros_col: self
                .p95_micros_col
                .reader()
                .expect("Failed to open column reader"),
            p99_micros_col: self
                .p99_micros_col
                .reader()
                .expect("Failed to open column reader"),
            queue_depth_col: self
                .queue_depth_col
                .reader()
                .expect("Failed to open column reader"),
            open_conns_col: self
                .open_conns_col
                .reader()
                .expect("Failed to open column reader"),
            gc_pause_micros_col: self
                .gc_pause_micros_col
                .reader()
                .expect("Failed to open column reader"),
            uptime_secs_col: self
                .uptime_secs_col
                .reader()
                .expect("Failed to open column reader"),
            temp_celsius_col: self
                .temp_celsius_col
                .reader()
                .expect("Failed to open column reader"),
            throttled_col: self
                .throttled_col
                .reader()
                .expect("Failed to open column reader"),
            healthy_col: self
                .healthy_col
                .reader()
                .expect("Failed to open column reader"),
            device_id_index: self.device_id_index.clone(),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open tombstones reader"),
        }
    }
}
pub struct MetricStorageReader {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    id_col: forgedb_storage::FixedColumnReader,
    recorded_at_col: forgedb_storage::FixedColumnReader,
    device_id_col: forgedb_storage::FixedColumnReader,
    sample_seq_col: forgedb_storage::FixedColumnReader,
    region_col: forgedb_storage::FixedColumnReader,
    cpu_pct_col: forgedb_storage::FixedColumnReader,
    mem_pct_col: forgedb_storage::FixedColumnReader,
    disk_pct_col: forgedb_storage::FixedColumnReader,
    net_rx_bytes_col: forgedb_storage::FixedColumnReader,
    net_tx_bytes_col: forgedb_storage::FixedColumnReader,
    req_count_col: forgedb_storage::FixedColumnReader,
    err_count_col: forgedb_storage::FixedColumnReader,
    p50_micros_col: forgedb_storage::FixedColumnReader,
    p95_micros_col: forgedb_storage::FixedColumnReader,
    p99_micros_col: forgedb_storage::FixedColumnReader,
    queue_depth_col: forgedb_storage::FixedColumnReader,
    open_conns_col: forgedb_storage::FixedColumnReader,
    gc_pause_micros_col: forgedb_storage::FixedColumnReader,
    uptime_secs_col: forgedb_storage::FixedColumnReader,
    temp_celsius_col: forgedb_storage::FixedColumnReader,
    throttled_col: forgedb_storage::FixedColumnReader,
    healthy_col: forgedb_storage::FixedColumnReader,
    device_id_index: std::sync::Arc<
        std::collections::HashMap<String, std::collections::HashSet<Uuid>>,
    >,
    tombstones: forgedb_storage::TombstonesReader,
}
impl MetricStorageReader {
    pub fn read_at(&self, row_index: usize) -> Option<Metric> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let recorded_at_value = Timestamp::from(
            self
                .recorded_at_col
                .read_timestamp(row_index)
                .expect("Failed to read from column"),
        );
        let device_id_value = self
            .device_id_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let sample_seq_value = self
            .sample_seq_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let region_value = self
            .region_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let cpu_pct_value = self
            .cpu_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let mem_pct_value = self
            .mem_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let disk_pct_value = self
            .disk_pct_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let net_rx_bytes_value = self
            .net_rx_bytes_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let net_tx_bytes_value = self
            .net_tx_bytes_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let req_count_value = self
            .req_count_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let err_count_value = self
            .err_count_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let p50_micros_value = self
            .p50_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let p95_micros_value = self
            .p95_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let p99_micros_value = self
            .p99_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let queue_depth_value = self
            .queue_depth_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let open_conns_value = self
            .open_conns_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let gc_pause_micros_value = self
            .gc_pause_micros_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let uptime_secs_value = self
            .uptime_secs_col
            .read_i64(row_index)
            .expect("Failed to read from column");
        let temp_celsius_value = self
            .temp_celsius_col
            .read_f64(row_index)
            .expect("Failed to read from column");
        let throttled_value = self
            .throttled_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        let healthy_value = self
            .healthy_col
            .read_bool(row_index)
            .expect("Failed to read from column");
        Some(Metric {
            id: id_value,
            recorded_at: recorded_at_value,
            device_id: device_id_value,
            sample_seq: sample_seq_value,
            region: region_value,
            cpu_pct: cpu_pct_value,
            mem_pct: mem_pct_value,
            disk_pct: disk_pct_value,
            net_rx_bytes: net_rx_bytes_value,
            net_tx_bytes: net_tx_bytes_value,
            req_count: req_count_value,
            err_count: err_count_value,
            p50_micros: p50_micros_value,
            p95_micros: p95_micros_value,
            p99_micros: p99_micros_value,
            queue_depth: queue_depth_value,
            open_conns: open_conns_value,
            gc_pause_micros: gc_pause_micros_value,
            uptime_secs: uptime_secs_value,
            temp_celsius: temp_celsius_value,
            throttled: throttled_value,
            healthy: healthy_value,
        })
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Metric> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Metric> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn find_by_device_id_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        value: u64,
    ) -> Vec<Metric> {
        let __k: String = {
            {
                let __v = &(value);
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            }
        };
        let __ids = match self.device_id_index.get(&__k) {
            Some(__s) => __s,
            None => return Vec::new(),
        };
        let mut __out = Vec::new();
        for &__id in __ids {
            if let Some(__rec) = self.get_at(snap, __id) {
                let __rk: String = {
                    {
                        let __v = &(__rec.device_id);
                        use std::fmt::Write as _;
                        let mut __k = String::from('\u{2}');
                        let _ = write!(__k, "{}", __v);
                        __k
                    }
                };
                if __rk == __k {
                    __out.push(__rec);
                }
            }
        }
        __out
    }
}
fn validate_metric(record: &Metric) -> Result<(), ValidationError> {
    Ok(())
}
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Doc {
    #[serde(default)]
    pub id: Uuid,
    pub seq: u64,
    pub kind: u32,
    pub body_a: String,
    pub body_b: String,
    pub body_c: String,
    pub body_d: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocMeta {
    pub id: Uuid,
    pub seq: u64,
    pub kind: u32,
}
#[derive(Debug, Clone)]
pub struct DocScanRef<'a> {
    pub __slot: usize,
    pub id: Uuid,
    pub seq: u64,
    pub kind: u32,
    pub body_a: &'a str,
    pub body_b: &'a str,
    pub body_c: &'a str,
    pub body_d: &'a str,
}
#[derive(serde::Serialize)]
pub struct DocPageRef<'a> {
    pub id: Uuid,
    pub seq: u64,
    pub kind: u32,
    pub body_a: &'a str,
    pub body_b: &'a str,
    pub body_c: &'a str,
    pub body_d: &'a str,
}
pub struct DocStorage {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    row_count: usize,
    id_col: FixedColumn,
    seq_col: FixedColumn,
    kind_col: FixedColumn,
    body_a_col: VariableColumn,
    body_b_col: VariableColumn,
    body_c_col: VariableColumn,
    body_d_col: VariableColumn,
    tombstones: forgedb_storage::Tombstones,
    wal: forgedb_wal::WalManager,
    writes_since_checkpoint: u64,
    root: std::path::PathBuf,
    dead_since_compaction: u64,
    in_transaction: bool,
    checkpoint_deferred: bool,
    compact_deferred: bool,
    compaction_due: bool,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl DocStorage {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("doc/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            seq_col: FixedColumn::new(root.join("doc/fixed/u64_1.bin"), 8usize)
                .expect("Failed to create fixed column"),
            kind_col: FixedColumn::new(root.join("doc/fixed/u32_2.bin"), 4usize)
                .expect("Failed to create fixed column"),
            body_a_col: VariableColumn::new(
                    root.join("doc/variable/string_data_3.bin"),
                    root.join("doc/variable/string_offsets_3.bin"),
                )
                .expect("Failed to create variable column"),
            body_b_col: VariableColumn::new(
                    root.join("doc/variable/string_data_4.bin"),
                    root.join("doc/variable/string_offsets_4.bin"),
                )
                .expect("Failed to create variable column"),
            body_c_col: VariableColumn::new(
                    root.join("doc/variable/string_data_5.bin"),
                    root.join("doc/variable/string_offsets_5.bin"),
                )
                .expect("Failed to create variable column"),
            body_d_col: VariableColumn::new(
                    root.join("doc/variable/string_data_6.bin"),
                    root.join("doc/variable/string_offsets_6.bin"),
                )
                .expect("Failed to create variable column"),
            tombstones: forgedb_storage::Tombstones::new(root.join("doc/tombstones.bin"))
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("doc/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let n = db.tombstones.len();
        db.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = db.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut db.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut db.id_versions).entry(id).or_default().push(i);
        }
        let _ = db.write_manifest(root);
        db
    }
    fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
        let mut db = Self {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            id_versions: std::sync::Arc::new(HashMap::new()),
            row_count: 0,
            id_col: FixedColumn::new(root.join("doc/fixed/uuid_0.bin"), 16usize)
                .expect("Failed to create fixed column"),
            seq_col: FixedColumn::new(root.join("doc/fixed/u64_1.bin"), 8usize)
                .expect("Failed to create fixed column"),
            kind_col: FixedColumn::new(root.join("doc/fixed/u32_2.bin"), 4usize)
                .expect("Failed to create fixed column"),
            body_a_col: VariableColumn::new(
                    root.join("doc/variable/string_data_3.bin"),
                    root.join("doc/variable/string_offsets_3.bin"),
                )
                .expect("Failed to create variable column"),
            body_b_col: VariableColumn::new(
                    root.join("doc/variable/string_data_4.bin"),
                    root.join("doc/variable/string_offsets_4.bin"),
                )
                .expect("Failed to create variable column"),
            body_c_col: VariableColumn::new(
                    root.join("doc/variable/string_data_5.bin"),
                    root.join("doc/variable/string_offsets_5.bin"),
                )
                .expect("Failed to create variable column"),
            body_d_col: VariableColumn::new(
                    root.join("doc/variable/string_data_6.bin"),
                    root.join("doc/variable/string_offsets_6.bin"),
                )
                .expect("Failed to create variable column"),
            tombstones: forgedb_storage::Tombstones::new(root.join("doc/tombstones.bin"))
                .expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                    root.join("doc/wal.log"),
                    forgedb_wal::FsyncPolicy::Always,
                )
                .expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        };
        db.recover_from_wal();
        let _ = db.write_manifest(root);
        db
    }
    fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "id".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/uuid_0.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "seq".to_string(), column_type : forgedb_storage::ColumnType::U64,
            column_index : 1usize, value_size : 8usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/u64_1.bin"
            .to_string(), }, forgedb_storage::ColumnMetadata { name : "kind".to_string(),
            column_type : forgedb_storage::ColumnType::U32, column_index : 2usize,
            value_size : 4usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path
            : "fixed/u32_2.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "body_a".to_string(), column_type : forgedb_storage::ColumnType::String,
            column_index : 3usize, value_size : 0usize, kind :
            forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_3.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "body_b".to_string(), column_type :
            forgedb_storage::ColumnType::String, column_index : 4usize, value_size :
            0usize, kind : forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_4.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "body_c".to_string(), column_type :
            forgedb_storage::ColumnType::String, column_index : 5usize, value_size :
            0usize, kind : forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_5.bin".to_string(), }, forgedb_storage::ColumnMetadata
            { name : "body_d".to_string(), column_type :
            forgedb_storage::ColumnType::String, column_index : 6usize, value_size :
            0usize, kind : forgedb_storage::ColumnKind::Variable, relative_path :
            "variable/string_data_6.bin".to_string(), }
        ];
        let __manifest_abs = root.join("doc/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.auto_sequences)
            .unwrap_or_default();
        #[allow(unused_mut)]
        let mut __auto_sequences = __persisted_seqs;
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "tombstones.bin".to_string(),
                bytes_per_row: 1usize,
            }),
            auto_sequences: __auto_sequences,
        };
        manifest.save_to(&__manifest_abs)
    }
    fn bump_compaction_epoch(&self, root: &std::path::Path) {
        let __manifest_abs = root.join("doc/manifest.json");
        if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
            __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
            let _ = __m.save_to(&__manifest_abs);
        }
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    pub fn insert(&mut self, record: Doc) -> Result<Uuid, ValidationError> {
        validate_doc(&record)?;
        let row_index = self.row_count;
        let id = record.id;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Doc", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.seq_col.append_u64(record.seq).expect("Failed to append to column");
        self.kind_col.append_u32(record.kind).expect("Failed to append to column");
        self.body_a_col.append_string(&record.body_a).expect("Failed to append string");
        self.body_b_col.append_string(&record.body_b).expect("Failed to append string");
        self.body_c_col.append_string(&record.body_c).expect("Failed to append string");
        self.body_d_col.append_string(&record.body_d).expect("Failed to append string");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(feed) = &self.changefeed {
            feed.emit("Doc", row_index, forgedb_changefeed::ChangeKind::Inserted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Doc",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Inserted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        Ok(id)
    }
    pub fn update(&mut self, id: Uuid, record: Doc) -> Result<bool, ValidationError> {
        if !self.id_to_row.contains_key(&id) {
            return Ok(false);
        }
        validate_doc(&record)?;
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(0u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Doc", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.seq_col.append_u64(record.seq).expect("Failed to append to column");
        self.kind_col.append_u32(record.kind).expect("Failed to append to column");
        self.body_a_col.append_string(&record.body_a).expect("Failed to append string");
        self.body_b_col.append_string(&record.body_b).expect("Failed to append string");
        self.body_c_col.append_string(&record.body_c).expect("Failed to append string");
        self.body_d_col.append_string(&record.body_d).expect("Failed to append string");
        self.tombstones.append(false).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(feed) = &self.changefeed {
            feed.emit("Doc", row_index, forgedb_changefeed::ChangeKind::Updated);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Doc",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Updated,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        Ok(true)
    }
    pub fn delete(&mut self, id: Uuid) -> bool {
        let record = match self.get(id) {
            Some(r) => r,
            None => return false,
        };
        let deleted_row = *self
            .id_to_row
            .get(&id)
            .expect("id present: get succeeded above");
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        {
            let mut __wal_payload = Vec::new();
            __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
            __wal_payload.push(1u8);
            __wal_payload.extend_from_slice(&__record_json);
            self.wal
                .write(&forgedb_wal::WalEntry::raw("Doc", __wal_payload))
                .expect("Failed to write WAL record");
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.seq_col.append_u64(record.seq).expect("Failed to append to column");
        self.kind_col.append_u32(record.kind).expect("Failed to append to column");
        self.body_a_col.append_string(&record.body_a).expect("Failed to append string");
        self.body_b_col.append_string(&record.body_b).expect("Failed to append string");
        self.body_c_col.append_string(&record.body_c).expect("Failed to append string");
        self.body_d_col.append_string(&record.body_d).expect("Failed to append string");
        self.tombstones.append(true).expect("Failed to append tombstone");
        std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
        std::sync::Arc::make_mut(&mut self.id_versions)
            .entry(id)
            .or_default()
            .push(row_index);
        self.row_count += 1;
        if let Some(feed) = &self.changefeed {
            feed.emit("Doc", deleted_row, forgedb_changefeed::ChangeKind::Deleted);
        }
        if let Some(__broker) = &self.broker {
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "Doc",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __record_json,
                    );
            }
        }
        self.writes_since_checkpoint += 1;
        if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
            if self.in_transaction {
                self.checkpoint_deferred = true;
            } else {
                self.checkpoint();
            }
        }
        self.dead_since_compaction += 1;
        if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
            if self.in_transaction {
                self.compact_deferred = true;
            } else if self.dead_since_compaction
                >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
            {
                self.compaction_due = false;
                self.compact();
            } else {
                self.compaction_due = true;
            }
        }
        true
    }
    pub fn apply(
        &mut self,
        kind: forgedb_changefeed::ChangeKind,
        bytes: &[u8],
    ) -> Result<(), ApplyError> {
        match kind {
            forgedb_changefeed::ChangeKind::Inserted => {
                let record: Doc = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                self.insert(record)?;
            }
            forgedb_changefeed::ChangeKind::Updated => {
                let record: Doc = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.update(id, record)?;
            }
            forgedb_changefeed::ChangeKind::Deleted => {
                let record: Doc = serde_json::from_slice(bytes)
                    .map_err(|e| ApplyError::Decode(e.to_string()))?;
                let id = record.id;
                self.delete(id);
            }
            forgedb_changefeed::ChangeKind::Linked => {}
        }
        Ok(())
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.id_col.sync_to_drive()?;
        self.seq_col.sync_to_drive()?;
        self.kind_col.sync_to_drive()?;
        self.body_a_col.sync_to_drive()?;
        self.body_b_col.sync_to_drive()?;
        self.body_c_col.sync_to_drive()?;
        self.body_d_col.sync_to_drive()?;
        self.tombstones.sync_to_drive()?;
        self.tombstones.barrier()?;
        Ok(())
    }
    fn recover_from_wal(&mut self) {
        let __anchor = self.tombstones.len();
        while self.id_col.len() < __anchor {
            self.id_col.append_uuid([0u8; 16]).expect("Failed to backfill column");
        }
        while self.seq_col.len() < __anchor {
            self.seq_col.append_u64(0).expect("Failed to backfill column");
        }
        while self.kind_col.len() < __anchor {
            self.kind_col.append_u32(0).expect("Failed to backfill column");
        }
        while self.body_a_col.len() < __anchor {
            self.body_a_col.append_string("").expect("Failed to backfill string column");
        }
        while self.body_b_col.len() < __anchor {
            self.body_b_col.append_string("").expect("Failed to backfill string column");
        }
        while self.body_c_col.len() < __anchor {
            self.body_c_col.append_string("").expect("Failed to backfill string column");
        }
        while self.body_d_col.len() < __anchor {
            self.body_d_col.append_string("").expect("Failed to backfill string column");
        }
        self.id_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.seq_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.kind_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.body_a_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.body_b_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.body_c_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.body_d_col
            .truncate_to_rows(__anchor)
            .expect("Failed to truncate column on recovery");
        self.row_count = __anchor;
        let __entries = self
            .wal
            .replay(|_| -> std::io::Result<()> { Ok(()) })
            .expect("Failed to replay WAL on recovery");
        for __entry in &__entries {
            if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                if payload.len() < 9 {
                    continue;
                }
                let mut __ri = [0u8; 8];
                __ri.copy_from_slice(&payload[0..8]);
                let __row_index = u64::from_le_bytes(__ri) as usize;
                if __row_index < self.row_count {
                    continue;
                }
                let __deleted = payload[8] != 0;
                let record: Doc = match serde_json::from_slice(&payload[9..]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                self.id_col
                    .append_uuid(*record.id.as_bytes())
                    .expect("Failed to append to column");
                self.seq_col.append_u64(record.seq).expect("Failed to append to column");
                self.kind_col
                    .append_u32(record.kind)
                    .expect("Failed to append to column");
                self.body_a_col
                    .append_string(&record.body_a)
                    .expect("Failed to append string");
                self.body_b_col
                    .append_string(&record.body_b)
                    .expect("Failed to append string");
                self.body_c_col
                    .append_string(&record.body_c)
                    .expect("Failed to append string");
                self.body_d_col
                    .append_string(&record.body_d)
                    .expect("Failed to append string");
                self.tombstones
                    .append(__deleted)
                    .expect("Failed to append tombstone on recovery");
                self.row_count += 1;
            }
        }
    }
    pub fn checkpoint(&mut self) {
        self.id_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.seq_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.kind_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.body_a_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.body_b_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.body_c_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.body_d_col
            .sync_to_drive()
            .expect("Failed to sync column to drive on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync tombstones to drive on checkpoint");
        self.tombstones.barrier().expect("Failed to issue checkpoint device barrier");
        self.wal.truncate().expect("Failed to truncate WAL on checkpoint");
        self.writes_since_checkpoint = 0;
    }
    pub fn compact(&mut self) {
        if self.in_transaction {
            self.compact_deferred = true;
            return;
        }
        self.checkpoint();
        let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
        for &__row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                __keep.push(__row);
            }
        }
        let __config = forgedb_compaction::CompactionConfig::default();
        let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
        if __compactor.compact_model_keeping("doc", &__keep).is_ok() {
            let mut __keep_sorted = __keep;
            __keep_sorted.sort_unstable();
            let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
            let __root = self.root.clone();
            let __feed = self.changefeed.take();
            let __broker = self.broker.take();
            *self = Self::new_at_no_rehydrate(&__root);
            self.changefeed = __feed;
            self.broker = __broker;
            let mut __new_id_to_row: std::collections::HashMap<_, usize> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> = std::collections::HashMap::with_capacity(
                __old_id_to_row.len(),
            );
            for (&__id, &__old_row) in __old_id_to_row.iter() {
                if __keep_sorted.binary_search(&__old_row).is_ok() {
                    let __new_row = __keep_sorted
                        .partition_point(|&__r| __r < __old_row);
                    __new_id_to_row.insert(__id, __new_row);
                    __new_id_versions.insert(__id, vec![__new_row]);
                }
            }
            self.id_to_row = std::sync::Arc::new(__new_id_to_row);
            self.id_versions = std::sync::Arc::new(__new_id_versions);
            self.bump_compaction_epoch(&__root);
        }
        self.dead_since_compaction = 0;
        self.compaction_due = false;
    }
    pub fn maintain(&mut self) {
        if self.compaction_due && !self.in_transaction {
            self.compaction_due = false;
            self.compact();
        }
    }
    pub fn __reindex_committed(&mut self) {
        std::sync::Arc::make_mut(&mut self.id_to_row).clear();
        std::sync::Arc::make_mut(&mut self.id_versions).clear();
        let n = self.tombstones.len();
        self.row_count = n;
        for i in 0..n {
            let id = {
                let bytes = self.id_col.read_uuid(i).expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, i);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(i);
        }
    }
    pub fn __reindex_delta(&mut self, from: usize) {
        let __n = self.row_count;
        for __r in from..__n {
            let id = {
                let bytes = self
                    .id_col
                    .read_uuid(__r)
                    .expect("Failed to read id column");
                Uuid::from_bytes(bytes)
            };
            let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
            std::sync::Arc::make_mut(&mut self.id_versions)
                .entry(id)
                .or_default()
                .push(__r);
            if !__deleted {}
        }
    }
    pub fn __sync_columns_from_disk(&mut self) {
        let _ = self.id_col.sync_from_disk();
        let _ = self.seq_col.sync_from_disk();
        let _ = self.kind_col.sync_from_disk();
        let _ = self.body_a_col.sync_from_disk();
        let _ = self.body_b_col.sync_from_disk();
        let _ = self.body_c_col.sync_from_disk();
        let _ = self.body_d_col.sync_from_disk();
        let _ = self.tombstones.sync_from_disk();
        self.row_count = self.tombstones.len();
    }
    pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
        self.id_col.truncate_to_rows(rows)?;
        self.seq_col.truncate_to_rows(rows)?;
        self.kind_col.truncate_to_rows(rows)?;
        self.body_a_col.truncate_to_rows(rows)?;
        self.body_b_col.truncate_to_rows(rows)?;
        self.body_c_col.truncate_to_rows(rows)?;
        self.body_d_col.truncate_to_rows(rows)?;
        self.tombstones.truncate_to_rows(rows)?;
        Ok(())
    }
    pub fn __recover_to_committed(&mut self, committed_len: usize) {
        if committed_len >= self.row_count {
            return;
        }
        self.id_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.seq_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.kind_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.body_a_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.body_b_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.body_c_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.body_d_col
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate column on journal recovery");
        self.tombstones
            .truncate_to_rows(committed_len)
            .expect("Failed to truncate tombstones on journal recovery");
        self.row_count = committed_len;
        self.wal.truncate().expect("Failed to truncate WAL on journal recovery");
        let __root = self.root.clone();
        let __feed = self.changefeed.take();
        let __broker = self.broker.take();
        *self = Self::new_at(&__root);
        self.changefeed = __feed;
        self.broker = __broker;
    }
    pub fn __stage_append(&mut self, record: Doc, deleted: bool) -> usize {
        let row_index = self.row_count;
        let __record_json = serde_json::to_vec(&record)
            .expect("Failed to serialize record");
        if deleted {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(1u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Doc", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        } else {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(0u8);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .write_buffered(&forgedb_wal::WalEntry::raw("Doc", __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
        self.id_col
            .append_uuid(*record.id.as_bytes())
            .expect("Failed to append to column");
        self.seq_col.append_u64(record.seq).expect("Failed to append to column");
        self.kind_col.append_u32(record.kind).expect("Failed to append to column");
        self.body_a_col.append_string(&record.body_a).expect("Failed to append string");
        self.body_b_col.append_string(&record.body_b).expect("Failed to append string");
        self.body_c_col.append_string(&record.body_c).expect("Failed to append string");
        self.body_d_col.append_string(&record.body_d).expect("Failed to append string");
        self.tombstones.append(deleted).expect("Failed to append tombstone");
        self.row_count += 1;
        row_index
    }
    pub fn run_deferred_maintenance(&mut self) {
        if self.compact_deferred {
            self.compact_deferred = false;
            self.checkpoint_deferred = false;
            self.compact();
        } else if self.checkpoint_deferred {
            self.checkpoint_deferred = false;
            self.checkpoint();
        }
    }
    pub fn read_at(&self, row_index: usize) -> Option<Doc> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let seq_value = self
            .seq_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let kind_value = self
            .kind_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let body_a_value = self
            .body_a_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_b_value = self
            .body_b_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_c_value = self
            .body_c_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_d_value = self
            .body_d_col
            .read_string(row_index)
            .expect("Failed to read string");
        Some(Doc {
            id: id_value,
            seq: seq_value,
            kind: kind_value,
            body_a: body_a_value,
            body_b: body_b_value,
            body_c: body_c_value,
            body_d: body_d_value,
        })
    }
    pub fn get(&self, id: Uuid) -> Option<Doc> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_at(row_index)
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Doc> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Doc> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
    pub fn all(&self) -> Vec<Doc> {
        let mut records = Vec::new();
        for id in self.id_to_row.keys() {
            if let Some(record) = self.get(*id) {
                records.push(record);
            }
        }
        records
    }
    pub fn export_live_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        for &row in self.id_to_row.values() {
            if !self.tombstones.is_deleted(row).unwrap_or(true) {
                indices.push(row);
            }
        }
        indices.sort_unstable();
        indices
    }
    pub fn export_col_id(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.id_col.export(indices)
    }
    pub fn export_col_seq(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.seq_col.export(indices)
    }
    pub fn export_col_kind(
        &self,
        indices: &[usize],
    ) -> std::io::Result<forgedb_storage::ColumnExport> {
        self.kind_col.export(indices)
    }
    fn __proj_resolve_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<usize> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        Some(versions[pos - 1])
    }
    fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
        let watermark = snap.watermark();
        let mut rows = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            rows.push(versions[pos - 1]);
        }
        rows
    }
    pub fn read_meta_at(&self, row_index: usize) -> Option<DocMeta> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let seq_value = self
            .seq_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let kind_value = self
            .kind_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        Some(DocMeta {
            id: id_value,
            seq: seq_value,
            kind: kind_value,
        })
    }
    pub fn get_meta(&self, id: Uuid) -> Option<DocMeta> {
        let row_index = *self.id_to_row.get(&id)?;
        self.read_meta_at(row_index)
    }
    pub fn all_meta(&self) -> Vec<DocMeta> {
        let mut __rows: Vec<usize> = self.id_to_row.values().copied().collect();
        __rows.sort_unstable();
        let __rows = self
            .tombstones
            .live_indices(&__rows)
            .expect("Failed to read tombstone liveness");
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __DocMetaScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            seq_col: forgedb_storage::BufferedFixedColumn,
            kind_col: forgedb_storage::BufferedFixedColumn,
        }
        let __bufs = __DocMetaScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            seq_col: self
                .seq_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            kind_col: self
                .kind_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut rows = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let seq_value = __bufs
                .seq_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let kind_value = __bufs
                .kind_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            rows.push(DocMeta {
                id: id_value,
                seq: seq_value,
                kind: kind_value,
            });
        }
        rows
    }
    pub fn get_meta_at(
        &self,
        snap: &forgedb_storage::Snapshot,
        id: Uuid,
    ) -> Option<DocMeta> {
        let row_index = self.__proj_resolve_at(snap, id)?;
        self.read_meta_at(row_index)
    }
    pub fn all_meta_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<DocMeta> {
        let mut records = Vec::new();
        for row_index in self.__proj_live_rows_at(snap) {
            if let Some(record) = self.read_meta_at(row_index) {
                records.push(record);
            }
        }
        records
    }
    pub fn __with_scan<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&DocScanRef<'_>) -> bool,
        f: impl FnOnce(&mut Vec<DocScanRef<'_>>) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __DocScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            seq_col: forgedb_storage::BufferedFixedColumn,
            kind_col: forgedb_storage::BufferedFixedColumn,
            body_a_col: forgedb_storage::BufferedVariableColumn,
            body_b_col: forgedb_storage::BufferedVariableColumn,
            body_c_col: forgedb_storage::BufferedVariableColumn,
            body_d_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __DocScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            seq_col: self
                .seq_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            kind_col: self
                .kind_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_a_col: self
                .body_a_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_b_col: self
                .body_b_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_c_col: self
                .body_c_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_d_col: self
                .body_d_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<DocScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let seq_value = __bufs
                .seq_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let kind_value = __bufs
                .kind_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let body_a_value = __bufs
                .body_a_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_b_value = __bufs
                .body_b_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_c_value = __bufs
                .body_c_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_d_value = __bufs
                .body_d_col
                .read_str(__slot)
                .expect("Failed to read string");
            let __row_ref = DocScanRef {
                __slot,
                id: id_value,
                seq: seq_value,
                kind: kind_value,
                body_a: body_a_value,
                body_b: body_b_value,
                body_c: body_c_value,
                body_d: body_d_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        f(&mut __refs)
    }
    pub fn __with_page<R>(
        &self,
        sel: Option<Vec<usize>>,
        keep: impl Fn(&DocScanRef<'_>) -> bool,
        sort: impl FnOnce(&mut Vec<DocScanRef<'_>>),
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[DocPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = match sel {
            Some(mut __c) => {
                __c.sort_unstable();
                __c
            }
            None => {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones
                    .live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        };
        let __n = __rows.len();
        #[allow(non_camel_case_types)]
        struct __DocPageScanBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            seq_col: forgedb_storage::BufferedFixedColumn,
            kind_col: forgedb_storage::BufferedFixedColumn,
            body_a_col: forgedb_storage::BufferedVariableColumn,
            body_b_col: forgedb_storage::BufferedVariableColumn,
            body_c_col: forgedb_storage::BufferedVariableColumn,
            body_d_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __DocPageScanBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            seq_col: self
                .seq_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            kind_col: self
                .kind_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_a_col: self
                .body_a_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_b_col: self
                .body_b_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_c_col: self
                .body_c_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
            body_d_col: self
                .body_d_col
                .gather_buffered(&__rows)
                .expect("Failed to bulk-load scan column"),
        };
        let mut __refs: Vec<DocScanRef<'_>> = Vec::with_capacity(__n);
        for __slot in 0..__n {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__slot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let seq_value = __bufs
                .seq_col
                .read_u64(__slot)
                .expect("Failed to read from column");
            let kind_value = __bufs
                .kind_col
                .read_u32(__slot)
                .expect("Failed to read from column");
            let body_a_value = __bufs
                .body_a_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_b_value = __bufs
                .body_b_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_c_value = __bufs
                .body_c_col
                .read_str(__slot)
                .expect("Failed to read string");
            let body_d_value = __bufs
                .body_d_col
                .read_str(__slot)
                .expect("Failed to read string");
            let __row_ref = DocScanRef {
                __slot,
                id: id_value,
                seq: seq_value,
                kind: kind_value,
                body_a: body_a_value,
                body_b: body_b_value,
                body_c: body_c_value,
                body_d: body_d_value,
            };
            if keep(&__row_ref) {
                __refs.push(__row_ref);
            }
        }
        let __total = __refs.len();
        sort(&mut __refs);
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        let __page = &__refs[__start..__end];
        let __page_rows: Vec<usize> = __page
            .iter()
            .map(|__r| __rows[__r.__slot])
            .collect();
        #[allow(non_camel_case_types)]
        struct __DocPageBufs {}
        let __page_bufs = __DocPageBufs {};
        let mut __views: Vec<DocPageRef<'_>> = Vec::with_capacity(__page.len());
        for (__pslot, __ref) in __page.iter().enumerate() {
            __views
                .push(DocPageRef {
                    id: __ref.id,
                    seq: __ref.seq,
                    kind: __ref.kind,
                    body_a: __ref.body_a,
                    body_b: __ref.body_b,
                    body_c: __ref.body_c,
                    body_d: __ref.body_d,
                });
        }
        f(__total, &__views)
    }
    pub fn __with_fast_page<R>(
        &self,
        offset: usize,
        limit: usize,
        f: impl FnOnce(usize, &[DocPageRef<'_>]) -> R,
    ) -> R {
        let __rows: Vec<usize> = {
            let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
            __all.sort_unstable();
            self.tombstones
                .live_indices(&__all)
                .expect("Failed to read tombstone liveness")
        };
        let __total = __rows.len();
        let __start = offset.min(__total);
        let __end = offset.saturating_add(limit).min(__total);
        #[allow(non_camel_case_types)]
        struct __DocFastPageBufs {
            id_col: forgedb_storage::BufferedFixedColumn,
            seq_col: forgedb_storage::BufferedFixedColumn,
            kind_col: forgedb_storage::BufferedFixedColumn,
            body_a_col: forgedb_storage::BufferedVariableColumn,
            body_b_col: forgedb_storage::BufferedVariableColumn,
            body_c_col: forgedb_storage::BufferedVariableColumn,
            body_d_col: forgedb_storage::BufferedVariableColumn,
        }
        let __bufs = __DocFastPageBufs {
            id_col: self
                .id_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            seq_col: self
                .seq_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            kind_col: self
                .kind_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            body_a_col: self
                .body_a_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            body_b_col: self
                .body_b_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            body_c_col: self
                .body_c_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
            body_d_col: self
                .body_d_col
                .gather_buffered(&__rows[__start..__end])
                .expect("Failed to bulk-load fast-page column"),
        };
        let mut __views: Vec<DocPageRef<'_>> = Vec::with_capacity(__end - __start);
        for __pslot in 0..(__end - __start) {
            let id_value = {
                let bytes = __bufs
                    .id_col
                    .read_uuid(__pslot)
                    .expect("Failed to read from column");
                Uuid::from_bytes(bytes)
            };
            let seq_value = __bufs
                .seq_col
                .read_u64(__pslot)
                .expect("Failed to read from column");
            let kind_value = __bufs
                .kind_col
                .read_u32(__pslot)
                .expect("Failed to read from column");
            let body_a_value = __bufs
                .body_a_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let body_b_value = __bufs
                .body_b_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let body_c_value = __bufs
                .body_c_col
                .read_str(__pslot)
                .expect("Failed to read string");
            let body_d_value = __bufs
                .body_d_col
                .read_str(__pslot)
                .expect("Failed to read string");
            __views
                .push(DocPageRef {
                    id: id_value,
                    seq: seq_value,
                    kind: kind_value,
                    body_a: body_a_value,
                    body_b: body_b_value,
                    body_c: body_c_value,
                    body_d: body_d_value,
                });
        }
        f(__total, &__views)
    }
    pub fn reader(&self) -> DocStorageReader {
        DocStorageReader {
            id_to_row: self.id_to_row.clone(),
            id_versions: self.id_versions.clone(),
            id_col: self.id_col.reader().expect("Failed to open column reader"),
            seq_col: self.seq_col.reader().expect("Failed to open column reader"),
            kind_col: self.kind_col.reader().expect("Failed to open column reader"),
            body_a_col: self.body_a_col.reader().expect("Failed to open column reader"),
            body_b_col: self.body_b_col.reader().expect("Failed to open column reader"),
            body_c_col: self.body_c_col.reader().expect("Failed to open column reader"),
            body_d_col: self.body_d_col.reader().expect("Failed to open column reader"),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open tombstones reader"),
        }
    }
}
pub struct DocStorageReader {
    id_to_row: std::sync::Arc<HashMap<Uuid, usize>>,
    id_versions: std::sync::Arc<HashMap<Uuid, Vec<usize>>>,
    id_col: forgedb_storage::FixedColumnReader,
    seq_col: forgedb_storage::FixedColumnReader,
    kind_col: forgedb_storage::FixedColumnReader,
    body_a_col: forgedb_storage::VariableColumnReader,
    body_b_col: forgedb_storage::VariableColumnReader,
    body_c_col: forgedb_storage::VariableColumnReader,
    body_d_col: forgedb_storage::VariableColumnReader,
    tombstones: forgedb_storage::TombstonesReader,
}
impl DocStorageReader {
    pub fn read_at(&self, row_index: usize) -> Option<Doc> {
        if self.tombstones.is_deleted(row_index).unwrap_or(true) {
            return None;
        }
        let id_value = {
            let bytes = self
                .id_col
                .read_uuid(row_index)
                .expect("Failed to read from column");
            Uuid::from_bytes(bytes)
        };
        let seq_value = self
            .seq_col
            .read_u64(row_index)
            .expect("Failed to read from column");
        let kind_value = self
            .kind_col
            .read_u32(row_index)
            .expect("Failed to read from column");
        let body_a_value = self
            .body_a_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_b_value = self
            .body_b_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_c_value = self
            .body_c_col
            .read_string(row_index)
            .expect("Failed to read string");
        let body_d_value = self
            .body_d_col
            .read_string(row_index)
            .expect("Failed to read string");
        Some(Doc {
            id: id_value,
            seq: seq_value,
            kind: kind_value,
            body_a: body_a_value,
            body_b: body_b_value,
            body_c: body_c_value,
            body_d: body_d_value,
        })
    }
    pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: Uuid) -> Option<Doc> {
        let watermark = snap.watermark();
        let versions = self.id_versions.get(&id)?;
        let pos = versions.partition_point(|&r| r < watermark);
        if pos == 0 {
            return None;
        }
        self.read_at(versions[pos - 1])
    }
    pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<Doc> {
        let watermark = snap.watermark();
        let mut records = Vec::new();
        for versions in self.id_versions.values() {
            let pos = versions.partition_point(|&r| r < watermark);
            if pos == 0 {
                continue;
            }
            if let Some(record) = self.read_at(versions[pos - 1]) {
                records.push(record);
            }
        }
        records
    }
}
fn validate_doc(record: &Doc) -> Result<(), ValidationError> {
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInserted {
    pub user: User,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserUpdated {
    pub user: User,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserDeleted {
    pub user: User,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostInserted {
    pub post: Post,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostUpdated {
    pub post: Post,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostDeleted {
    pub post: Post,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TagInserted {
    pub tag: Tag,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TagUpdated {
    pub tag: Tag,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TagDeleted {
    pub tag: Tag,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricInserted {
    pub metric: Metric,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricUpdated {
    pub metric: Metric,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricDeleted {
    pub metric: Metric,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocInserted {
    pub doc: Doc,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocUpdated {
    pub doc: Doc,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocDeleted {
    pub doc: Doc,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum UserLiveDelta {
    Init { rows: Vec<User> },
    Added { row: User },
    Updated { row: User },
    Removed { id: Uuid },
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PostLiveDelta {
    Init { rows: Vec<Post> },
    Added { row: Post },
    Updated { row: Post },
    Removed { id: Uuid },
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TagLiveDelta {
    Init { rows: Vec<Tag> },
    Added { row: Tag },
    Updated { row: Tag },
    Removed { id: Uuid },
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MetricLiveDelta {
    Init { rows: Vec<Metric> },
    Added { row: Metric },
    Updated { row: Metric },
    Removed { id: Uuid },
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DocLiveDelta {
    Init { rows: Vec<Doc> },
    Added { row: Doc },
    Updated { row: Doc },
    Removed { id: Uuid },
}
pub struct PostTagLink {
    left_col: FixedColumn,
    right_col: FixedColumn,
    tombstones: Tombstones,
    row_count: usize,
    left_index: std::collections::HashMap<Uuid, Vec<Uuid>>,
    right_index: std::collections::HashMap<Uuid, Vec<Uuid>>,
    changefeed: Option<forgedb_changefeed::ChangeFeed>,
    broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
}
impl PostTagLink {
    pub fn new() -> Self {
        Self::new_at(std::path::Path::new("."))
    }
    pub fn new_at(root: &std::path::Path) -> Self {
        let left_col = FixedColumn::new(
                root.join("post_tag_link/fixed/left.bin"),
                16usize,
            )
            .expect("Failed to create junction column");
        let right_col = FixedColumn::new(
                root.join("post_tag_link/fixed/right.bin"),
                16usize,
            )
            .expect("Failed to create junction column");
        let mut tombstones = Tombstones::new(root.join("post_tag_link/tombstones.bin"))
            .expect("Failed to create junction tombstones");
        let row_count = right_col.len();
        while tombstones.len() < row_count {
            tombstones.append(false).expect("Failed to back-fill junction tombstone");
        }
        let mut left_index: std::collections::HashMap<Uuid, Vec<Uuid>> = std::collections::HashMap::new();
        let mut right_index: std::collections::HashMap<Uuid, Vec<Uuid>> = std::collections::HashMap::new();
        {
            let mut __state: std::collections::HashMap<(Uuid, Uuid), bool> = std::collections::HashMap::new();
            let mut __order: Vec<(Uuid, Uuid)> = Vec::new();
            for i in 0..row_count {
                let l = Uuid::from_bytes(
                    left_col.read_uuid(i).expect("Failed to read link"),
                );
                let r = Uuid::from_bytes(
                    right_col.read_uuid(i).expect("Failed to read link"),
                );
                if !__state.contains_key(&(l, r)) {
                    __order.push((l, r));
                }
                __state.insert((l, r), tombstones.is_deleted(i).unwrap_or(false));
            }
            for (l, r) in __order {
                if !__state.get(&(l, r)).copied().unwrap_or(true) {
                    left_index.entry(l).or_default().push(r);
                    right_index.entry(r).or_default().push(l);
                }
            }
        }
        let db = Self {
            left_col,
            right_col,
            tombstones,
            row_count,
            left_index,
            right_index,
            changefeed: None,
            broker: None,
        };
        db.write_manifest(root);
        db
    }
    fn __index_add(&mut self, left: Uuid, right: Uuid) {
        let rs = self.left_index.entry(left).or_default();
        if !rs.contains(&right) {
            rs.push(right);
        }
        let ls = self.right_index.entry(right).or_default();
        if !ls.contains(&left) {
            ls.push(left);
        }
    }
    fn __index_remove(&mut self, left: Uuid, right: Uuid) {
        if let Some(rs) = self.left_index.get_mut(&left) {
            rs.retain(|r| *r != right);
            if rs.is_empty() {
                self.left_index.remove(&left);
            }
        }
        if let Some(ls) = self.right_index.get_mut(&right) {
            ls.retain(|l| *l != left);
            if ls.is_empty() {
                self.right_index.remove(&right);
            }
        }
    }
    pub fn rights_of(&self, left: Uuid) -> Vec<Uuid> {
        self.left_index.get(&left).cloned().unwrap_or_default()
    }
    pub fn lefts_of(&self, right: Uuid) -> Vec<Uuid> {
        self.right_index.get(&right).cloned().unwrap_or_default()
    }
    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
        self.changefeed = Some(feed);
    }
    pub fn attach_broker(
        &mut self,
        broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        >,
    ) {
        self.broker = broker;
    }
    fn write_manifest(&self, root: &std::path::Path) {
        let columns = vec![
            forgedb_storage::ColumnMetadata { name : "left".to_string(), column_type :
            forgedb_storage::ColumnType::Uuid, column_index : 0usize, value_size :
            16usize, kind : forgedb_storage::ColumnKind::Fixed, relative_path :
            "fixed/left.bin".to_string(), }, forgedb_storage::ColumnMetadata { name :
            "right".to_string(), column_type : forgedb_storage::ColumnType::Uuid,
            column_index : 1usize, value_size : 16usize, kind :
            forgedb_storage::ColumnKind::Fixed, relative_path : "fixed/right.bin"
            .to_string(), },
        ];
        let __manifest_abs = root.join("post_tag_link/manifest.json");
        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.compaction_epoch)
            .unwrap_or(0);
        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
            .map(|m| m.schema_version)
            .unwrap_or(EXPECTED_SCHEMA_VERSION);
        let manifest = forgedb_storage::Manifest {
            row_count: self.row_count,
            columns,
            wal_enabled: false,
            last_checkpoint: self.row_count as u64,
            compaction_epoch: __compaction_epoch,
            schema_version: __schema_version,
            engine_version: EXPECTED_ENGINE_VERSION,
            row_anchor: Some(forgedb_storage::RowAnchor {
                relative_path: "fixed/right.bin".to_string(),
                bytes_per_row: 16usize,
            }),
            auto_sequences: Default::default(),
        };
        let _ = manifest.save_to(&__manifest_abs);
    }
    pub fn link(&mut self, left: Uuid, right: Uuid) {
        let row_index = self.row_count;
        self.left_col.append_uuid(*left.as_bytes()).expect("Failed to append link");
        self.right_col.append_uuid(*right.as_bytes()).expect("Failed to append link");
        self.tombstones.append(false).expect("Failed to append junction tombstone");
        self.row_count += 1;
        self.__index_add(left, right);
        if let Some(feed) = &self.changefeed {
            feed.emit(
                "post_tag_link",
                row_index,
                forgedb_changefeed::ChangeKind::Linked,
            );
        }
        if let Some(__broker) = &self.broker {
            let mut __row_bytes = Vec::with_capacity(32);
            __row_bytes.extend_from_slice(left.as_bytes());
            __row_bytes.extend_from_slice(right.as_bytes());
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "post_tag_link",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Linked,
                        __row_bytes,
                    );
            }
        }
    }
    pub fn unlink(&mut self, left: Uuid, right: Uuid) -> bool {
        let __live = self
            .left_index
            .get(&left)
            .map(|rs| rs.contains(&right))
            .unwrap_or(false);
        if !__live {
            return false;
        }
        let row_index = self.row_count;
        self.left_col.append_uuid(*left.as_bytes()).expect("Failed to append unlink");
        self.right_col.append_uuid(*right.as_bytes()).expect("Failed to append unlink");
        self.tombstones.append(true).expect("Failed to append junction tombstone");
        self.row_count += 1;
        self.__index_remove(left, right);
        if let Some(feed) = &self.changefeed {
            feed.emit(
                "post_tag_link",
                row_index,
                forgedb_changefeed::ChangeKind::Deleted,
            );
        }
        if let Some(__broker) = &self.broker {
            let mut __row_bytes = Vec::with_capacity(32);
            __row_bytes.extend_from_slice(left.as_bytes());
            __row_bytes.extend_from_slice(right.as_bytes());
            if let Ok(mut __b) = __broker.lock() {
                let _ = __b
                    .record(
                        "post_tag_link",
                        row_index as u64,
                        forgedb_changefeed::ChangeKind::Deleted,
                        __row_bytes,
                    );
            }
        }
        true
    }
    pub fn unlink_all_left(&mut self, id: Uuid) {
        let __targets = self.rights_of(id);
        for r in __targets {
            self.unlink(id, r);
        }
    }
    pub fn unlink_all_right(&mut self, id: Uuid) {
        let __targets = self.lefts_of(id);
        for l in __targets {
            self.unlink(l, id);
        }
    }
    pub fn checkpoint(&mut self) {
        self.left_col
            .sync_to_drive()
            .expect("Failed to sync junction left column on checkpoint");
        self.right_col
            .sync_to_drive()
            .expect("Failed to sync junction right column on checkpoint");
        self.tombstones
            .sync_to_drive()
            .expect("Failed to sync junction tombstones on checkpoint");
        self.tombstones
            .barrier()
            .expect("Failed to issue junction checkpoint device barrier");
    }
    pub fn pairs(&self) -> Vec<(Uuid, Uuid)> {
        self.pairs_prefix(self.row_count)
    }
    fn pairs_prefix(&self, end: usize) -> Vec<(Uuid, Uuid)> {
        let mut order: Vec<(Uuid, Uuid)> = Vec::new();
        let mut state: std::collections::HashMap<(Uuid, Uuid), bool> = std::collections::HashMap::new();
        for i in 0..end {
            let left = Uuid::from_bytes(
                self.left_col.read_uuid(i).expect("Failed to read link"),
            );
            let right = Uuid::from_bytes(
                self.right_col.read_uuid(i).expect("Failed to read link"),
            );
            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
            if !state.contains_key(&(left, right)) {
                order.push((left, right));
            }
            state.insert((left, right), deleted);
        }
        order
            .into_iter()
            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
            .collect()
    }
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
        forgedb_storage::Snapshot::new(self.row_count)
    }
    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)> {
        self.pairs_prefix(snap.watermark())
    }
    pub fn reader(&self) -> PostTagLinkReader {
        PostTagLinkReader {
            left_col: self
                .left_col
                .reader()
                .expect("Failed to open junction left reader"),
            right_col: self
                .right_col
                .reader()
                .expect("Failed to open junction right reader"),
            tombstones: self
                .tombstones
                .reader()
                .expect("Failed to open junction tombstones reader"),
        }
    }
}
pub struct PostTagLinkReader {
    left_col: forgedb_storage::FixedColumnReader,
    right_col: forgedb_storage::FixedColumnReader,
    tombstones: forgedb_storage::TombstonesReader,
}
impl PostTagLinkReader {
    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)> {
        let end = snap.watermark();
        let mut order: Vec<(Uuid, Uuid)> = Vec::new();
        let mut state: std::collections::HashMap<(Uuid, Uuid), bool> = std::collections::HashMap::new();
        for i in 0..end {
            let left = Uuid::from_bytes(
                self.left_col.read_uuid(i).expect("Failed to read link"),
            );
            let right = Uuid::from_bytes(
                self.right_col.read_uuid(i).expect("Failed to read link"),
            );
            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
            if !state.contains_key(&(left, right)) {
                order.push((left, right));
            }
            state.insert((left, right), deleted);
        }
        order
            .into_iter()
            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
            .collect()
    }
}
pub struct Database {
    pub user: UserStorage,
    pub post: PostStorage,
    pub tag: TagStorage,
    pub metric: MetricStorage,
    pub doc: DocStorage,
    pub post_tag_link: PostTagLink,
    pub changefeed: forgedb_changefeed::ChangeFeed,
    pub broker: Option<
        std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
    >,
    _lock: Option<forgedb_storage::DirLock>,
    _txn_journal: Option<forgedb_wal::WalManager>,
    seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
}
pub struct DatabaseReader {
    pub user: UserStorageReader,
    pub post: PostStorageReader,
    pub tag: TagStorageReader,
    pub metric: MetricStorageReader,
    pub doc: DocStorageReader,
    pub post_tag_link: PostTagLinkReader,
}
pub struct DatabaseSnapshot {
    pub user: forgedb_storage::Snapshot,
    pub post: forgedb_storage::Snapshot,
    pub tag: forgedb_storage::Snapshot,
    pub metric: forgedb_storage::Snapshot,
    pub doc: forgedb_storage::Snapshot,
    pub post_tag_link: forgedb_storage::Snapshot,
}
impl Database {
    pub fn new() -> Self {
        let changefeed = forgedb_changefeed::ChangeFeed::new(1024);
        let mut user = UserStorage::new();
        user.attach_changefeed(changefeed.clone());
        let mut post = PostStorage::new();
        post.attach_changefeed(changefeed.clone());
        let mut tag = TagStorage::new();
        tag.attach_changefeed(changefeed.clone());
        let mut metric = MetricStorage::new();
        metric.attach_changefeed(changefeed.clone());
        let mut doc = DocStorage::new();
        doc.attach_changefeed(changefeed.clone());
        let mut post_tag_link = PostTagLink::new();
        post_tag_link.attach_changefeed(changefeed.clone());
        Self {
            user,
            post,
            tag,
            metric,
            doc,
            post_tag_link,
            changefeed,
            broker: None,
            _lock: None,
            _txn_journal: None,
            seq: std::sync::Arc::new(
                std::sync::Mutex::new(forgedb_txn::CommitSequencer::new(0)),
            ),
        }
    }
    pub fn open_at(root: std::path::PathBuf) -> Self {
        let _lock = Some(
            forgedb_storage::DirLock::acquire(&root)
                .expect(
                    "another writer already holds this data dir \
                             (ForgeDB is single-writer-per-process, #89)",
                ),
        );
        Self::__open_with_lock(root, _lock)
    }
    fn __open_with_lock(
        root: std::path::PathBuf,
        _lock: Option<forgedb_storage::DirLock>,
    ) -> Self {
        {
            let __mf = root.join("user/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        {
            let __mf = root.join("post/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        {
            let __mf = root.join("tag/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        {
            let __mf = root.join("metric/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        {
            let __mf = root.join("doc/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        {
            let __mf = root.join("post_tag_link/manifest.json");
            if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                        __mf.display(), __m.schema_version, EXPECTED_SCHEMA_VERSION,
                    );
                }
                if __m.engine_version != EXPECTED_ENGINE_VERSION {
                    panic!(
                        "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                        __mf.display(), __m.engine_version, EXPECTED_ENGINE_VERSION,
                    );
                }
            }
        }
        let changefeed = forgedb_changefeed::ChangeFeed::new(1024);
        let broker: Option<
            std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
        > = None;
        let __seq_start = match &broker {
            Some(b) => b.lock().map(|b| b.watermark()).unwrap_or(0),
            None => 0,
        };
        let seq = std::sync::Arc::new(
            std::sync::Mutex::new(forgedb_txn::CommitSequencer::new(__seq_start)),
        );
        let mut _txn_journal = forgedb_wal::WalManager::open(
                root.join("_txn_journal.log"),
                forgedb_wal::FsyncPolicy::Always,
            )
            .expect("Failed to open transaction journal");
        let __journal_committed: Vec<(String, u64)> = {
            let __entries = _txn_journal
                .replay(|_| -> std::io::Result<()> { Ok(()) })
                .unwrap_or_default();
            match __entries.last() {
                Some(__e) => {
                    if let forgedb_wal::WalOperation::Raw { payload } = &__e.operation {
                        serde_json::from_slice(payload).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
                None => Vec::new(),
            }
        };
        let mut user = UserStorage::new_at(&root);
        user.attach_changefeed(changefeed.clone());
        user.attach_broker(broker.clone());
        let mut post = PostStorage::new_at(&root);
        post.attach_changefeed(changefeed.clone());
        post.attach_broker(broker.clone());
        let mut tag = TagStorage::new_at(&root);
        tag.attach_changefeed(changefeed.clone());
        tag.attach_broker(broker.clone());
        let mut metric = MetricStorage::new_at(&root);
        metric.attach_changefeed(changefeed.clone());
        metric.attach_broker(broker.clone());
        let mut doc = DocStorage::new_at(&root);
        doc.attach_changefeed(changefeed.clone());
        doc.attach_broker(broker.clone());
        let mut post_tag_link = PostTagLink::new_at(&root);
        post_tag_link.attach_changefeed(changefeed.clone());
        post_tag_link.attach_broker(broker.clone());
        let mut __db = Self {
            user,
            post,
            tag,
            metric,
            doc,
            post_tag_link,
            changefeed,
            broker,
            _lock,
            _txn_journal: Some(_txn_journal),
            seq,
        };
        for (__model, __len) in &__journal_committed {
            let __len = *__len;
            match __model.as_str() {
                "User" => __db.user.__recover_to_committed(__len as usize),
                "Post" => __db.post.__recover_to_committed(__len as usize),
                "Tag" => __db.tag.__recover_to_committed(__len as usize),
                "Metric" => __db.metric.__recover_to_committed(__len as usize),
                "Doc" => __db.doc.__recover_to_committed(__len as usize),
                _ => {}
            }
        }
        __db
    }
    pub fn snapshot(&self) -> DatabaseSnapshot {
        DatabaseSnapshot {
            user: self.user.snapshot(),
            post: self.post.snapshot(),
            tag: self.tag.snapshot(),
            metric: self.metric.snapshot(),
            doc: self.doc.snapshot(),
            post_tag_link: self.post_tag_link.snapshot(),
        }
    }
    pub fn checkpoint(&mut self) {
        self.user.checkpoint();
        self.post.checkpoint();
        self.tag.checkpoint();
        self.metric.checkpoint();
        self.doc.checkpoint();
        self.post_tag_link.checkpoint();
    }
    pub fn compact(&mut self) {
        if let Ok(__seq) = self.seq.lock() {
            if __seq.oldest_live_snapshot().as_u64() > 0 {
                return;
            }
        }
        self.user.compact();
        self.post.compact();
        self.tag.compact();
        self.metric.compact();
        self.doc.compact();
    }
    pub fn maintain(&mut self) {
        if let Ok(__seq) = self.seq.lock() {
            if __seq.oldest_live_snapshot().as_u64() > 0 {
                return;
            }
        }
        self.user.maintain();
        self.post.maintain();
        self.tag.maintain();
        self.metric.maintain();
        self.doc.maintain();
    }
    pub fn reader(&self) -> DatabaseReader {
        DatabaseReader {
            user: self.user.reader(),
            post: self.post.reader(),
            tag: self.tag.reader(),
            metric: self.metric.reader(),
            doc: self.doc.reader(),
            post_tag_link: self.post_tag_link.reader(),
        }
    }
    pub fn apply_frame(
        &mut self,
        ev: &forgedb_changefeed::durable::PersistedEvent,
    ) -> Result<(), ApplyError> {
        match ev.model.as_str() {
            "User" => self.user.apply(ev.kind, &ev.bytes),
            "Post" => self.post.apply(ev.kind, &ev.bytes),
            "Tag" => self.tag.apply(ev.kind, &ev.bytes),
            "Metric" => self.metric.apply(ev.kind, &ev.bytes),
            "Doc" => self.doc.apply(ev.kind, &ev.bytes),
            "post_tag_link" => {
                if ev.bytes.len() == 32 {
                    let __l = {
                        let mut __b = [0u8; 16];
                        __b.copy_from_slice(&ev.bytes[0..16]);
                        Uuid::from_bytes(__b)
                    };
                    let __r = {
                        let mut __b = [0u8; 16];
                        __b.copy_from_slice(&ev.bytes[16..32]);
                        Uuid::from_bytes(__b)
                    };
                    self.post_tag_link.link(__l, __r);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn recover_to(
        &mut self,
        base_offset: u64,
        target_offset: u64,
    ) -> Result<u64, ApplyError> {
        let broker = self
            .broker
            .take()
            .ok_or_else(|| ApplyError::Decode(
                "no replication broker: recover_to needs a data dir \
                             opened via open_at with a _replication.log present"
                    .to_string(),
            ))?;
        self.user.attach_broker(None);
        self.post.attach_broker(None);
        self.tag.attach_broker(None);
        self.metric.attach_broker(None);
        self.doc.attach_broker(None);
        self.post_tag_link.attach_broker(None);
        const BATCH: usize = 512;
        let mut after = base_offset;
        let mut applied = base_offset;
        let result: Result<u64, ApplyError> = (|| {
            loop {
                let frames = {
                    let guard = broker.lock().unwrap();
                    guard
                        .read_from(after, BATCH)
                        .map_err(|e| ApplyError::Decode(e.to_string()))?
                };
                if frames.is_empty() {
                    break;
                }
                for ev in &frames {
                    if ev.offset > target_offset {
                        return Ok(applied);
                    }
                    self.apply_frame(ev)?;
                    applied = ev.offset;
                    after = ev.offset;
                }
            }
            Ok(applied)
        })();
        self.user.attach_broker(Some(broker.clone()));
        self.post.attach_broker(Some(broker.clone()));
        self.tag.attach_broker(Some(broker.clone()));
        self.metric.attach_broker(Some(broker.clone()));
        self.doc.attach_broker(Some(broker.clone()));
        self.post_tag_link.attach_broker(Some(broker.clone()));
        self.broker = Some(broker);
        let applied = result?;
        self.commit().map_err(|e| ApplyError::Decode(e.to_string()))?;
        Ok(applied)
    }
    pub fn commit(&mut self) -> std::io::Result<()> {
        self.user.commit()?;
        self.post.commit()?;
        self.tag.commit()?;
        self.metric.commit()?;
        self.doc.commit()?;
        self.post_tag_link.checkpoint();
        Ok(())
    }
}
impl Database {
    pub fn post_author(&self, record: &Post) -> Option<User> {
        self.user.get(record.author)
    }
    pub fn user_posts(&self, id: Uuid) -> Vec<Post> {
        self.post.find_by_author(id)
    }
    pub fn link_post_tag(&mut self, left: Uuid, right: Uuid) {
        self.post_tag_link.link(left, right);
    }
    pub fn unlink_post_tag(&mut self, left: Uuid, right: Uuid) -> bool {
        self.post_tag_link.unlink(left, right)
    }
    pub fn post_tags(&self, id: Uuid) -> Vec<Tag> {
        self.post_tag_link
            .rights_of(id)
            .into_iter()
            .filter_map(|right| self.tag.get(right))
            .collect()
    }
    pub fn post_tags_at(&self, snap: &DatabaseSnapshot, id: Uuid) -> Vec<Tag> {
        self.post_tag_link
            .pairs_at(&snap.post_tag_link)
            .into_iter()
            .filter(|(left, _)| *left == id)
            .filter_map(|(_, right)| { self.tag.get_at(&snap.tag, right) })
            .collect()
    }
    pub fn tag_posts(&self, id: Uuid) -> Vec<Post> {
        self.post_tag_link
            .lefts_of(id)
            .into_iter()
            .filter_map(|left| self.post.get(left))
            .collect()
    }
}
const MAX_CASCADE_DEPTH: u32 = 64;
impl Database {
    pub fn create_user(&mut self, mut record: User) -> Result<Uuid, ValidationError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        self.user.insert(record)
    }
    pub fn update_user(
        &mut self,
        id: Uuid,
        record: User,
    ) -> Result<bool, ValidationError> {
        self.user.update(id, record)
    }
    pub fn create_post(&mut self, mut record: Post) -> Result<Uuid, ValidationError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        if self.user.get(record.author).is_none() {
            return Err(ValidationError::DanglingReference {
                model: "Post",
                field: "author",
                target: "User",
            });
        }
        self.post.insert(record)
    }
    pub fn update_post(
        &mut self,
        id: Uuid,
        record: Post,
    ) -> Result<bool, ValidationError> {
        if self.user.get(record.author).is_none() {
            return Err(ValidationError::DanglingReference {
                model: "Post",
                field: "author",
                target: "User",
            });
        }
        self.post.update(id, record)
    }
    pub fn create_tag(&mut self, mut record: Tag) -> Result<Uuid, ValidationError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        self.tag.insert(record)
    }
    pub fn update_tag(
        &mut self,
        id: Uuid,
        record: Tag,
    ) -> Result<bool, ValidationError> {
        self.tag.update(id, record)
    }
    pub fn create_metric(
        &mut self,
        mut record: Metric,
    ) -> Result<Uuid, ValidationError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.recorded_at.as_micros() == 0 {
            record.recorded_at = Timestamp::now().floor_to_micros(1000);
        }
        self.metric.insert(record)
    }
    pub fn update_metric(
        &mut self,
        id: Uuid,
        record: Metric,
    ) -> Result<bool, ValidationError> {
        self.metric.update(id, record)
    }
    pub fn create_doc(&mut self, mut record: Doc) -> Result<Uuid, ValidationError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        self.doc.insert(record)
    }
    pub fn update_doc(
        &mut self,
        id: Uuid,
        record: Doc,
    ) -> Result<bool, ValidationError> {
        self.doc.update(id, record)
    }
    pub fn delete_user(&mut self, id: Uuid) -> Result<bool, ValidationError> {
        self.delete_user_cascade(id, 0)
    }
    fn delete_user_cascade(
        &mut self,
        id: Uuid,
        __depth: u32,
    ) -> Result<bool, ValidationError> {
        if __depth > MAX_CASCADE_DEPTH {
            return Err(ValidationError::ReferencedByChildren {
                model: "User",
                field: "on_delete cascade depth exceeded",
            });
        }
        if self.user.get(id).is_none() {
            return Ok(false);
        }
        {
            let __children = self.post.find_by_author(id);
            if !__children.is_empty() {
                return Err(ValidationError::ReferencedByChildren {
                    model: "Post",
                    field: "author",
                });
            }
        }
        Ok(self.user.delete(id))
    }
    pub fn delete_post(&mut self, id: Uuid) -> Result<bool, ValidationError> {
        self.delete_post_cascade(id, 0)
    }
    fn delete_post_cascade(
        &mut self,
        id: Uuid,
        __depth: u32,
    ) -> Result<bool, ValidationError> {
        if __depth > MAX_CASCADE_DEPTH {
            return Err(ValidationError::ReferencedByChildren {
                model: "Post",
                field: "on_delete cascade depth exceeded",
            });
        }
        if self.post.get(id).is_none() {
            return Ok(false);
        }
        self.post_tag_link.unlink_all_left(id);
        Ok(self.post.delete(id))
    }
    pub fn delete_tag(&mut self, id: Uuid) -> Result<bool, ValidationError> {
        self.delete_tag_cascade(id, 0)
    }
    fn delete_tag_cascade(
        &mut self,
        id: Uuid,
        __depth: u32,
    ) -> Result<bool, ValidationError> {
        if __depth > MAX_CASCADE_DEPTH {
            return Err(ValidationError::ReferencedByChildren {
                model: "Tag",
                field: "on_delete cascade depth exceeded",
            });
        }
        if self.tag.get(id).is_none() {
            return Ok(false);
        }
        self.post_tag_link.unlink_all_right(id);
        Ok(self.tag.delete(id))
    }
    pub fn delete_metric(&mut self, id: Uuid) -> Result<bool, ValidationError> {
        self.delete_metric_cascade(id, 0)
    }
    fn delete_metric_cascade(
        &mut self,
        id: Uuid,
        __depth: u32,
    ) -> Result<bool, ValidationError> {
        if __depth > MAX_CASCADE_DEPTH {
            return Err(ValidationError::ReferencedByChildren {
                model: "Metric",
                field: "on_delete cascade depth exceeded",
            });
        }
        if self.metric.get(id).is_none() {
            return Ok(false);
        }
        Ok(self.metric.delete(id))
    }
    pub fn delete_doc(&mut self, id: Uuid) -> Result<bool, ValidationError> {
        self.delete_doc_cascade(id, 0)
    }
    fn delete_doc_cascade(
        &mut self,
        id: Uuid,
        __depth: u32,
    ) -> Result<bool, ValidationError> {
        if __depth > MAX_CASCADE_DEPTH {
            return Err(ValidationError::ReferencedByChildren {
                model: "Doc",
                field: "on_delete cascade depth exceeded",
            });
        }
        if self.doc.get(id).is_none() {
            return Ok(false);
        }
        Ok(self.doc.delete(id))
    }
}
pub struct TxHandle<'db> {
    db: &'db mut Database,
    marks: std::collections::BTreeMap<&'static str, usize>,
    wal_marks: std::collections::BTreeMap<&'static str, u64>,
    pending_events: Vec<
        (&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, usize, Vec<u8>),
    >,
    staged_unique_keys: std::collections::BTreeSet<(&'static str, &'static str, String)>,
    committed: bool,
}
impl<'db> TxHandle<'db> {
    fn begin(db: &'db mut Database) -> Self {
        TxHandle {
            db,
            marks: std::collections::BTreeMap::new(),
            wal_marks: std::collections::BTreeMap::new(),
            pending_events: Vec::new(),
            staged_unique_keys: std::collections::BTreeSet::new(),
            committed: false,
        }
    }
    fn __mark_user(&mut self) {
        if !self.marks.contains_key("User") {
            let __rc = self.db.user.row_count;
            let __wb = self.db.user.wal.size().unwrap_or(0);
            self.marks.insert("User", __rc);
            self.wal_marks.insert("User", __wb);
            self.db.user.in_transaction = true;
        }
    }
    fn __mark_post(&mut self) {
        if !self.marks.contains_key("Post") {
            let __rc = self.db.post.row_count;
            let __wb = self.db.post.wal.size().unwrap_or(0);
            self.marks.insert("Post", __rc);
            self.wal_marks.insert("Post", __wb);
            self.db.post.in_transaction = true;
        }
    }
    fn __mark_tag(&mut self) {
        if !self.marks.contains_key("Tag") {
            let __rc = self.db.tag.row_count;
            let __wb = self.db.tag.wal.size().unwrap_or(0);
            self.marks.insert("Tag", __rc);
            self.wal_marks.insert("Tag", __wb);
            self.db.tag.in_transaction = true;
        }
    }
    fn __mark_metric(&mut self) {
        if !self.marks.contains_key("Metric") {
            let __rc = self.db.metric.row_count;
            let __wb = self.db.metric.wal.size().unwrap_or(0);
            self.marks.insert("Metric", __rc);
            self.wal_marks.insert("Metric", __wb);
            self.db.metric.in_transaction = true;
        }
    }
    fn __mark_doc(&mut self) {
        if !self.marks.contains_key("Doc") {
            let __rc = self.db.doc.row_count;
            let __wb = self.db.doc.wal.size().unwrap_or(0);
            self.marks.insert("Doc", __rc);
            self.wal_marks.insert("Doc", __wb);
            self.db.doc.in_transaction = true;
        }
    }
    pub fn create_user(&mut self, mut record: User) -> Result<Uuid, TxError> {
        self.__mark_user();
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.db.user.email_index.get(&__uk).is_some_and(|__ids| !__ids.is_empty())
            {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    }),
                );
            }
            if self.staged_unique_keys.contains(&("User", "email", __uk.clone())) {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    }),
                );
            }
            self.staged_unique_keys.insert(("User", "email", __uk));
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.user.__stage_append(record, false);
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_user(&mut self, id: Uuid, record: User) -> Result<bool, TxError> {
        if self.get_user(id).is_none() {
            return Ok(false);
        }
        self.__mark_user();
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if let Some(__ids) = self.db.user.email_index.get(&__uk) {
                if __ids.iter().any(|__i| *__i != id) {
                    return Err(
                        TxError::Validation(ValidationError::Unique {
                            model: "User",
                            field: "email",
                        }),
                    );
                }
            }
            let __committed_owns = self
                .db
                .user
                .email_index
                .get(&__uk)
                .is_some_and(|__ids| __ids.contains(&id));
            if !__committed_owns
                && self.staged_unique_keys.contains(&("User", "email", __uk.clone()))
            {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    }),
                );
            }
            self.staged_unique_keys.insert(("User", "email", __uk));
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.user.__stage_append(record, false);
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_user(&mut self, id: Uuid) -> bool {
        let record = match self.get_user(id) {
            Some(r) => r,
            None => return false,
        };
        self.__mark_user();
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.user.__stage_append(record, true);
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __row,
                __bytes,
            ));
        true
    }
    pub fn get_user(&self, id: Uuid) -> Option<User> {
        let __snap = forgedb_storage::Snapshot::new(self.db.user.row_count);
        self.db.user.get_at(&__snap, id)
    }
    pub fn all_user(&self) -> Vec<User> {
        let __snap = forgedb_storage::Snapshot::new(self.db.user.row_count);
        self.db.user.all_at(&__snap)
    }
    pub fn create_post(&mut self, mut record: Post) -> Result<Uuid, TxError> {
        self.__mark_post();
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        if self.get_user(record.author).is_none() {
            return Err(
                TxError::Validation(ValidationError::DanglingReference {
                    model: "Post",
                    field: "author",
                    target: "User",
                }),
            );
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.post.__stage_append(record, false);
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_post(&mut self, id: Uuid, record: Post) -> Result<bool, TxError> {
        if self.get_post(id).is_none() {
            return Ok(false);
        }
        self.__mark_post();
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        if self.get_user(record.author).is_none() {
            return Err(
                TxError::Validation(ValidationError::DanglingReference {
                    model: "Post",
                    field: "author",
                    target: "User",
                }),
            );
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.post.__stage_append(record, false);
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_post(&mut self, id: Uuid) -> bool {
        let record = match self.get_post(id) {
            Some(r) => r,
            None => return false,
        };
        self.__mark_post();
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.post.__stage_append(record, true);
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __row,
                __bytes,
            ));
        true
    }
    pub fn get_post(&self, id: Uuid) -> Option<Post> {
        let __snap = forgedb_storage::Snapshot::new(self.db.post.row_count);
        self.db.post.get_at(&__snap, id)
    }
    pub fn all_post(&self) -> Vec<Post> {
        let __snap = forgedb_storage::Snapshot::new(self.db.post.row_count);
        self.db.post.all_at(&__snap)
    }
    pub fn create_tag(&mut self, mut record: Tag) -> Result<Uuid, TxError> {
        self.__mark_tag();
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.db.tag.name_index.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    }),
                );
            }
            if self.staged_unique_keys.contains(&("Tag", "name", __uk.clone())) {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    }),
                );
            }
            self.staged_unique_keys.insert(("Tag", "name", __uk));
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.tag.__stage_append(record, false);
        self.pending_events
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_tag(&mut self, id: Uuid, record: Tag) -> Result<bool, TxError> {
        if self.get_tag(id).is_none() {
            return Ok(false);
        }
        self.__mark_tag();
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if let Some(__ids) = self.db.tag.name_index.get(&__uk) {
                if __ids.iter().any(|__i| *__i != id) {
                    return Err(
                        TxError::Validation(ValidationError::Unique {
                            model: "Tag",
                            field: "name",
                        }),
                    );
                }
            }
            let __committed_owns = self
                .db
                .tag
                .name_index
                .get(&__uk)
                .is_some_and(|__ids| __ids.contains(&id));
            if !__committed_owns
                && self.staged_unique_keys.contains(&("Tag", "name", __uk.clone()))
            {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    }),
                );
            }
            self.staged_unique_keys.insert(("Tag", "name", __uk));
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.tag.__stage_append(record, false);
        self.pending_events
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_tag(&mut self, id: Uuid) -> bool {
        let record = match self.get_tag(id) {
            Some(r) => r,
            None => return false,
        };
        self.__mark_tag();
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.tag.__stage_append(record, true);
        self.pending_events
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __row,
                __bytes,
            ));
        true
    }
    pub fn get_tag(&self, id: Uuid) -> Option<Tag> {
        let __snap = forgedb_storage::Snapshot::new(self.db.tag.row_count);
        self.db.tag.get_at(&__snap, id)
    }
    pub fn all_tag(&self) -> Vec<Tag> {
        let __snap = forgedb_storage::Snapshot::new(self.db.tag.row_count);
        self.db.tag.all_at(&__snap)
    }
    pub fn create_metric(&mut self, mut record: Metric) -> Result<Uuid, TxError> {
        self.__mark_metric();
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.recorded_at.as_micros() == 0 {
            record.recorded_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.metric.__stage_append(record, false);
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_metric(&mut self, id: Uuid, record: Metric) -> Result<bool, TxError> {
        if self.get_metric(id).is_none() {
            return Ok(false);
        }
        self.__mark_metric();
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.metric.__stage_append(record, false);
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_metric(&mut self, id: Uuid) -> bool {
        let record = match self.get_metric(id) {
            Some(r) => r,
            None => return false,
        };
        self.__mark_metric();
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.metric.__stage_append(record, true);
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __row,
                __bytes,
            ));
        true
    }
    pub fn get_metric(&self, id: Uuid) -> Option<Metric> {
        let __snap = forgedb_storage::Snapshot::new(self.db.metric.row_count);
        self.db.metric.get_at(&__snap, id)
    }
    pub fn all_metric(&self) -> Vec<Metric> {
        let __snap = forgedb_storage::Snapshot::new(self.db.metric.row_count);
        self.db.metric.all_at(&__snap)
    }
    pub fn create_doc(&mut self, mut record: Doc) -> Result<Uuid, TxError> {
        self.__mark_doc();
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        validate_doc(&record)?;
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.doc.__stage_append(record, false);
        self.pending_events
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_doc(&mut self, id: Uuid, record: Doc) -> Result<bool, TxError> {
        if self.get_doc(id).is_none() {
            return Ok(false);
        }
        self.__mark_doc();
        validate_doc(&record)?;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.doc.__stage_append(record, false);
        self.pending_events
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __row,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_doc(&mut self, id: Uuid) -> bool {
        let record = match self.get_doc(id) {
            Some(r) => r,
            None => return false,
        };
        self.__mark_doc();
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        let __row = self.db.doc.__stage_append(record, true);
        self.pending_events
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __row,
                __bytes,
            ));
        true
    }
    pub fn get_doc(&self, id: Uuid) -> Option<Doc> {
        let __snap = forgedb_storage::Snapshot::new(self.db.doc.row_count);
        self.db.doc.get_at(&__snap, id)
    }
    pub fn all_doc(&self) -> Vec<Doc> {
        let __snap = forgedb_storage::Snapshot::new(self.db.doc.row_count);
        self.db.doc.all_at(&__snap)
    }
    pub fn commit(mut self) -> Result<(), TxError> {
        let mut __journal: Vec<(String, u64)> = Vec::new();
        let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
        for __tag in &__touched {
            match *__tag {
                "User" => {
                    let _ = self.db.user.commit();
                    let _ = self.db.user.wal.flush();
                    __journal.push(("User".to_string(), self.db.user.row_count as u64));
                }
                "Post" => {
                    let _ = self.db.post.commit();
                    let _ = self.db.post.wal.flush();
                    __journal.push(("Post".to_string(), self.db.post.row_count as u64));
                }
                "Tag" => {
                    let _ = self.db.tag.commit();
                    let _ = self.db.tag.wal.flush();
                    __journal.push(("Tag".to_string(), self.db.tag.row_count as u64));
                }
                "Metric" => {
                    let _ = self.db.metric.commit();
                    let _ = self.db.metric.wal.flush();
                    __journal
                        .push(("Metric".to_string(), self.db.metric.row_count as u64));
                }
                "Doc" => {
                    let _ = self.db.doc.commit();
                    let _ = self.db.doc.wal.flush();
                    __journal.push(("Doc".to_string(), self.db.doc.row_count as u64));
                }
                _ => {}
            }
        }
        if let Some(__jrnl) = self.db._txn_journal.as_mut() {
            let __payload = serde_json::to_vec(&__journal)
                .map_err(|e| TxError::Io(e.to_string()))?;
            __jrnl
                .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                .map_err(|e| TxError::Io(e.to_string()))?;
            __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
        }
        for __tag in &__touched {
            match *__tag {
                "User" => {
                    self.db.user.__reindex_committed();
                    self.db.user.in_transaction = false;
                    self.db.user.run_deferred_maintenance();
                }
                "Post" => {
                    self.db.post.__reindex_committed();
                    self.db.post.in_transaction = false;
                    self.db.post.run_deferred_maintenance();
                }
                "Tag" => {
                    self.db.tag.__reindex_committed();
                    self.db.tag.in_transaction = false;
                    self.db.tag.run_deferred_maintenance();
                }
                "Metric" => {
                    self.db.metric.__reindex_committed();
                    self.db.metric.in_transaction = false;
                    self.db.metric.run_deferred_maintenance();
                }
                "Doc" => {
                    self.db.doc.__reindex_committed();
                    self.db.doc.in_transaction = false;
                    self.db.doc.run_deferred_maintenance();
                }
                _ => {}
            }
        }
        for (__model, __kind, _id_bytes, __row, __bytes) in std::mem::take(
            &mut self.pending_events,
        ) {
            self.db.changefeed.emit(__model, __row, __kind);
            if let Some(__broker) = &self.db.broker {
                if let Ok(mut __b) = __broker.lock() {
                    let _ = __b.record(__model, __row as u64, __kind, __bytes);
                }
            }
        }
        self.committed = true;
        Ok(())
    }
    pub fn rollback(mut self) {
        self.rollback_internal();
        self.committed = true;
    }
    fn rollback_internal(&mut self) {
        let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
        for __tag in &__touched {
            if let Some(__mark) = self.marks.get(*__tag) {
                match *__tag {
                    "User" => {
                        let __mark = *__mark;
                        let _ = self.db.user.__truncate_all_to(__mark);
                        self.db.user.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get("User") {
                            let _ = self.db.user.wal.truncate_to(*__wm);
                        }
                        self.db.user.in_transaction = false;
                        self.db.user.run_deferred_maintenance();
                    }
                    "Post" => {
                        let __mark = *__mark;
                        let _ = self.db.post.__truncate_all_to(__mark);
                        self.db.post.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get("Post") {
                            let _ = self.db.post.wal.truncate_to(*__wm);
                        }
                        self.db.post.in_transaction = false;
                        self.db.post.run_deferred_maintenance();
                    }
                    "Tag" => {
                        let __mark = *__mark;
                        let _ = self.db.tag.__truncate_all_to(__mark);
                        self.db.tag.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get("Tag") {
                            let _ = self.db.tag.wal.truncate_to(*__wm);
                        }
                        self.db.tag.in_transaction = false;
                        self.db.tag.run_deferred_maintenance();
                    }
                    "Metric" => {
                        let __mark = *__mark;
                        let _ = self.db.metric.__truncate_all_to(__mark);
                        self.db.metric.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get("Metric") {
                            let _ = self.db.metric.wal.truncate_to(*__wm);
                        }
                        self.db.metric.in_transaction = false;
                        self.db.metric.run_deferred_maintenance();
                    }
                    "Doc" => {
                        let __mark = *__mark;
                        let _ = self.db.doc.__truncate_all_to(__mark);
                        self.db.doc.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get("Doc") {
                            let _ = self.db.doc.wal.truncate_to(*__wm);
                        }
                        self.db.doc.in_transaction = false;
                        self.db.doc.run_deferred_maintenance();
                    }
                    _ => {}
                }
            }
        }
        self.pending_events.clear();
    }
    pub fn __write_set(&self, snapshot_lsn: forgedb_txn::Lsn) -> forgedb_txn::WriteSet {
        let mut keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
        for (__model, _kind, __id_bytes, _row, _bytes) in &self.pending_events {
            keys.push(
                __forgedb_ws_key(&[b"r", __model.as_bytes(), __id_bytes])
                    .into_boxed_slice(),
            );
        }
        for (__mtag, __fname, __ekey) in &self.staged_unique_keys {
            keys.push(
                __forgedb_ws_key(
                        &[b"u", __mtag.as_bytes(), __fname.as_bytes(), __ekey.as_bytes()],
                    )
                    .into_boxed_slice(),
            );
        }
        forgedb_txn::WriteSet {
            keys,
            snapshot_lsn,
        }
    }
}
impl<'db> Drop for TxHandle<'db> {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback_internal();
        }
    }
}
const DEFAULT_TXN_RETRIES: u32 = 3;
impl Database {
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&mut TxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        let mut tx = TxHandle::begin(self);
        match f(&mut tx) {
            Ok(v) => {
                tx.commit()?;
                Ok(v)
            }
            Err(e) => {
                tx.rollback();
                Err(e)
            }
        }
    }
    pub fn transaction_retrying<T>(
        &mut self,
        retries: u32,
        f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        let __seq_arc = std::sync::Arc::clone(&self.seq);
        for __attempt in 0..=retries {
            let __snap_lsn = {
                let mut __seq = self.seq.lock().unwrap();
                __seq.register_snapshot()
            };
            let mut tx = TxHandle::begin(self);
            let __out = f(&mut tx);
            let __val = match __out {
                Ok(v) => v,
                Err(e) => {
                    tx.rollback();
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    return Err(e);
                }
            };
            let __ws = tx.__write_set(__snap_lsn);
            let __outcome = {
                let mut __seq = __seq_arc.lock().unwrap();
                __seq.try_commit(&__ws)
            };
            match __outcome {
                forgedb_txn::CommitOutcome::Committed(_l) => {
                    tx.commit()?;
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    let _ = __attempt;
                    return Ok(__val);
                }
                forgedb_txn::CommitOutcome::Conflict { .. } => {
                    tx.rollback();
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                }
            }
        }
        Err(TxError::Conflict)
    }
    pub fn transaction_optimistic<T>(
        &mut self,
        f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        self.transaction_retrying(DEFAULT_TXN_RETRIES, f)
    }
}
#[derive(Clone)]
pub struct SharedDatabase {
    inner: std::sync::Arc<std::sync::RwLock<Database>>,
    seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
}
impl SharedDatabase {
    pub fn transaction_concurrent<T>(
        &self,
        retries: u32,
        f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        let __seq_arc = std::sync::Arc::clone(&self.seq);
        for __attempt in 0..=retries {
            let __snap_lsn = {
                let mut __seq = __seq_arc.lock().unwrap();
                __seq.register_snapshot()
            };
            let __snap = {
                let __db = self.inner.read().unwrap();
                __db.snapshot()
            };
            let mut __tx = ConcurrentTxHandle {
                inner: std::sync::Arc::clone(&self.inner),
                snap: __snap,
                buffer: Vec::new(),
                staged_unique_keys: std::collections::BTreeSet::new(),
                pending_events: Vec::new(),
            };
            let __out = f(&mut __tx);
            let __val = match __out {
                Ok(v) => v,
                Err(e) => {
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    return Err(e);
                }
            };
            let mut __ws_keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
            for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                __ws_keys
                    .push(
                        __forgedb_ws_key(&[b"r", __model.as_bytes(), __id_bytes])
                            .into_boxed_slice(),
                    );
            }
            for (__mtag, __fname, __ekey) in &__tx.staged_unique_keys {
                __ws_keys
                    .push(
                        __forgedb_ws_key(
                                &[
                                    b"u",
                                    __mtag.as_bytes(),
                                    __fname.as_bytes(),
                                    __ekey.as_bytes(),
                                ],
                            )
                            .into_boxed_slice(),
                    );
            }
            let __ws = forgedb_txn::WriteSet {
                keys: __ws_keys,
                snapshot_lsn: __snap_lsn,
            };
            let __outcome = {
                let mut __seq = __seq_arc.lock().unwrap();
                __seq.try_commit(&__ws)
            };
            match __outcome {
                forgedb_txn::CommitOutcome::Committed(_l) => {
                    {
                        let mut __db = self.inner.write().unwrap();
                        let __buf = __tx.buffer;
                        let __evts = __tx.pending_events;
                        __db.__apply_and_commit_concurrent_buffer(__buf, __evts)?;
                    }
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    let _ = __attempt;
                    return Ok(__val);
                }
                forgedb_txn::CommitOutcome::Conflict { .. } => {
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                }
            }
        }
        Err(TxError::Conflict)
    }
}
pub struct ConcurrentTxHandle {
    inner: std::sync::Arc<std::sync::RwLock<Database>>,
    snap: DatabaseSnapshot,
    buffer: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool)>,
    staged_unique_keys: std::collections::BTreeSet<(&'static str, &'static str, String)>,
    pending_events: Vec<
        (&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>),
    >,
}
impl ConcurrentTxHandle {
    pub fn create_user(&mut self, mut record: User) -> Result<Uuid, TxError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.staged_unique_keys.contains(&("User", "email", __uk.clone())) {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    }),
                );
            }
            {
                let __db = self.inner.read().unwrap();
                if __db
                    .user
                    .email_index
                    .get(&__uk)
                    .is_some_and(|__ids| !__ids.is_empty())
                {
                    return Err(
                        TxError::Validation(ValidationError::Unique {
                            model: "User",
                            field: "email",
                        }),
                    );
                }
            }
            self.staged_unique_keys.insert(("User", "email", __uk));
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_user(&mut self, id: Uuid, record: User) -> Result<bool, TxError> {
        if self.get_user(id).is_none() {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "User",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_user(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.email);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            {
                let __db = self.inner.read().unwrap();
                if let Some(__ids) = __db.user.email_index.get(&__uk) {
                    if __ids.iter().any(|__i| *__i != id) {
                        return Err(
                            TxError::Validation(ValidationError::Unique {
                                model: "User",
                                field: "email",
                            }),
                        );
                    }
                }
            }
            let __committed_owns = {
                let __db = self.inner.read().unwrap();
                __db.user.email_index.get(&__uk).is_some_and(|__ids| __ids.contains(&id))
            };
            if !__committed_owns
                && self.staged_unique_keys.contains(&("User", "email", __uk.clone()))
            {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "User",
                        field: "email",
                    }),
                );
            }
            self.staged_unique_keys.insert(("User", "email", __uk));
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_user(&mut self, id: Uuid) -> bool {
        let record = match self.get_user(id) {
            Some(r) => r,
            None => return false,
        };
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes.clone(),
                __bytes.clone(),
                true,
            ));
        self.pending_events
            .push((
                "User",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __bytes,
            ));
        true
    }
    pub fn get_user(&self, id: Uuid) -> Option<User> {
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
            if __ib == &__id_bytes {
                if *__del {
                    return None;
                }
                return serde_json::from_slice(__bytes).ok();
            }
        }
        let __db = self.inner.read().unwrap();
        __db.user.get_at(&self.snap.user, id)
    }
    pub fn all_user(&self) -> Vec<User> {
        let __db = self.inner.read().unwrap();
        let mut __rows = __db.user.all_at(&self.snap.user);
        drop(__db);
        for (_, _kind, __ib, __bytes, __del) in &self.buffer {
            let __id_bytes_ref: &Vec<u8> = __ib;
            if *__del {
                __rows
                    .retain(|r| {
                        serde_json::to_vec(&r.id).unwrap_or_default().as_slice()
                            != __id_bytes_ref.as_slice()
                    });
            } else if let Ok(__rec) = serde_json::from_slice::<User>(__bytes) {
                let __staged_id = __rec.id;
                if let Some(__pos) = __rows.iter().position(|r| r.id == __staged_id) {
                    __rows[__pos] = __rec;
                } else {
                    __rows.push(__rec);
                }
            }
        }
        __rows
    }
    pub fn create_post(&mut self, mut record: Post) -> Result<Uuid, TxError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.created_at.as_micros() == 0 {
            record.created_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        if self.get_user(record.author).is_none() {
            return Err(
                TxError::Validation(ValidationError::DanglingReference {
                    model: "Post",
                    field: "author",
                    target: "User",
                }),
            );
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_post(&mut self, id: Uuid, record: Post) -> Result<bool, TxError> {
        if self.get_post(id).is_none() {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.created_at = record.created_at.floor_to_micros(1000);
        if !record.created_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Post",
                field: "created_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.created_at.as_micros(),
                ),
            })?;
        }
        validate_post(&record)?;
        if self.get_user(record.author).is_none() {
            return Err(
                TxError::Validation(ValidationError::DanglingReference {
                    model: "Post",
                    field: "author",
                    target: "User",
                }),
            );
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_post(&mut self, id: Uuid) -> bool {
        let record = match self.get_post(id) {
            Some(r) => r,
            None => return false,
        };
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes.clone(),
                __bytes.clone(),
                true,
            ));
        self.pending_events
            .push((
                "Post",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __bytes,
            ));
        true
    }
    pub fn get_post(&self, id: Uuid) -> Option<Post> {
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
            if __ib == &__id_bytes {
                if *__del {
                    return None;
                }
                return serde_json::from_slice(__bytes).ok();
            }
        }
        let __db = self.inner.read().unwrap();
        __db.post.get_at(&self.snap.post, id)
    }
    pub fn all_post(&self) -> Vec<Post> {
        let __db = self.inner.read().unwrap();
        let mut __rows = __db.post.all_at(&self.snap.post);
        drop(__db);
        for (_, _kind, __ib, __bytes, __del) in &self.buffer {
            let __id_bytes_ref: &Vec<u8> = __ib;
            if *__del {
                __rows
                    .retain(|r| {
                        serde_json::to_vec(&r.id).unwrap_or_default().as_slice()
                            != __id_bytes_ref.as_slice()
                    });
            } else if let Ok(__rec) = serde_json::from_slice::<Post>(__bytes) {
                let __staged_id = __rec.id;
                if let Some(__pos) = __rows.iter().position(|r| r.id == __staged_id) {
                    __rows[__pos] = __rec;
                } else {
                    __rows.push(__rec);
                }
            }
        }
        __rows
    }
    pub fn create_tag(&mut self, mut record: Tag) -> Result<Uuid, TxError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            if self.staged_unique_keys.contains(&("Tag", "name", __uk.clone())) {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    }),
                );
            }
            {
                let __db = self.inner.read().unwrap();
                if __db.tag.name_index.get(&__uk).is_some_and(|__ids| !__ids.is_empty())
                {
                    return Err(
                        TxError::Validation(ValidationError::Unique {
                            model: "Tag",
                            field: "name",
                        }),
                    );
                }
            }
            self.staged_unique_keys.insert(("Tag", "name", __uk));
        }
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_tag(&mut self, id: Uuid, record: Tag) -> Result<bool, TxError> {
        if self.get_tag(id).is_none() {
            return Ok(false);
        }
        validate_tag(&record)?;
        {
            let __uk: String = {
                {
                    let __v = &(record.name);
                    let mut __k = String::with_capacity(1 + __v.len());
                    __k.push('\u{1}');
                    __k.push_str(__v);
                    __k
                }
            };
            {
                let __db = self.inner.read().unwrap();
                if let Some(__ids) = __db.tag.name_index.get(&__uk) {
                    if __ids.iter().any(|__i| *__i != id) {
                        return Err(
                            TxError::Validation(ValidationError::Unique {
                                model: "Tag",
                                field: "name",
                            }),
                        );
                    }
                }
            }
            let __committed_owns = {
                let __db = self.inner.read().unwrap();
                __db.tag.name_index.get(&__uk).is_some_and(|__ids| __ids.contains(&id))
            };
            if !__committed_owns
                && self.staged_unique_keys.contains(&("Tag", "name", __uk.clone()))
            {
                return Err(
                    TxError::Validation(ValidationError::Unique {
                        model: "Tag",
                        field: "name",
                    }),
                );
            }
            self.staged_unique_keys.insert(("Tag", "name", __uk));
        }
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push(("Tag", forgedb_changefeed::ChangeKind::Updated, __id_bytes, __bytes));
        Ok(true)
    }
    pub fn delete_tag(&mut self, id: Uuid) -> bool {
        let record = match self.get_tag(id) {
            Some(r) => r,
            None => return false,
        };
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Tag",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes.clone(),
                __bytes.clone(),
                true,
            ));
        self.pending_events
            .push(("Tag", forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __bytes));
        true
    }
    pub fn get_tag(&self, id: Uuid) -> Option<Tag> {
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
            if __ib == &__id_bytes {
                if *__del {
                    return None;
                }
                return serde_json::from_slice(__bytes).ok();
            }
        }
        let __db = self.inner.read().unwrap();
        __db.tag.get_at(&self.snap.tag, id)
    }
    pub fn all_tag(&self) -> Vec<Tag> {
        let __db = self.inner.read().unwrap();
        let mut __rows = __db.tag.all_at(&self.snap.tag);
        drop(__db);
        for (_, _kind, __ib, __bytes, __del) in &self.buffer {
            let __id_bytes_ref: &Vec<u8> = __ib;
            if *__del {
                __rows
                    .retain(|r| {
                        serde_json::to_vec(&r.id).unwrap_or_default().as_slice()
                            != __id_bytes_ref.as_slice()
                    });
            } else if let Ok(__rec) = serde_json::from_slice::<Tag>(__bytes) {
                let __staged_id = __rec.id;
                if let Some(__pos) = __rows.iter().position(|r| r.id == __staged_id) {
                    __rows[__pos] = __rec;
                } else {
                    __rows.push(__rec);
                }
            }
        }
        __rows
    }
    pub fn create_metric(&mut self, mut record: Metric) -> Result<Uuid, TxError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        if record.recorded_at.as_micros() == 0 {
            record.recorded_at = Timestamp::now().floor_to_micros(1000);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_metric(&mut self, id: Uuid, record: Metric) -> Result<bool, TxError> {
        if self.get_metric(id).is_none() {
            return Ok(false);
        }
        #[allow(unused_mut)]
        let mut record = record;
        record.recorded_at = record.recorded_at.floor_to_micros(1000);
        if !record.recorded_at.is_rfc3339_representable() {
            Err(ValidationError::Constraint {
                model: "Metric",
                field: "recorded_at",
                rule: "timestamp_range",
                message: format!(
                    "{} — got {} microseconds since the epoch",
                    "must be an instant RFC 3339 can name (years 0000 through 9999); the value is storable but would fail to serialize on every read",
                    record.recorded_at.as_micros(),
                ),
            })?;
        }
        validate_metric(&record)?;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes,
                __bytes,
            ));
        Ok(true)
    }
    pub fn delete_metric(&mut self, id: Uuid) -> bool {
        let record = match self.get_metric(id) {
            Some(r) => r,
            None => return false,
        };
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes.clone(),
                __bytes.clone(),
                true,
            ));
        self.pending_events
            .push((
                "Metric",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes,
                __bytes,
            ));
        true
    }
    pub fn get_metric(&self, id: Uuid) -> Option<Metric> {
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
            if __ib == &__id_bytes {
                if *__del {
                    return None;
                }
                return serde_json::from_slice(__bytes).ok();
            }
        }
        let __db = self.inner.read().unwrap();
        __db.metric.get_at(&self.snap.metric, id)
    }
    pub fn all_metric(&self) -> Vec<Metric> {
        let __db = self.inner.read().unwrap();
        let mut __rows = __db.metric.all_at(&self.snap.metric);
        drop(__db);
        for (_, _kind, __ib, __bytes, __del) in &self.buffer {
            let __id_bytes_ref: &Vec<u8> = __ib;
            if *__del {
                __rows
                    .retain(|r| {
                        serde_json::to_vec(&r.id).unwrap_or_default().as_slice()
                            != __id_bytes_ref.as_slice()
                    });
            } else if let Ok(__rec) = serde_json::from_slice::<Metric>(__bytes) {
                let __staged_id = __rec.id;
                if let Some(__pos) = __rows.iter().position(|r| r.id == __staged_id) {
                    __rows[__pos] = __rec;
                } else {
                    __rows.push(__rec);
                }
            }
        }
        __rows
    }
    pub fn create_doc(&mut self, mut record: Doc) -> Result<Uuid, TxError> {
        if record.id.is_nil() {
            record.id = Uuid::new_v4();
        }
        validate_doc(&record)?;
        let id = record.id;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Inserted,
                __id_bytes,
                __bytes,
            ));
        Ok(id)
    }
    pub fn update_doc(&mut self, id: Uuid, record: Doc) -> Result<bool, TxError> {
        if self.get_doc(id).is_none() {
            return Ok(false);
        }
        validate_doc(&record)?;
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Updated,
                __id_bytes.clone(),
                __bytes.clone(),
                false,
            ));
        self.pending_events
            .push(("Doc", forgedb_changefeed::ChangeKind::Updated, __id_bytes, __bytes));
        Ok(true)
    }
    pub fn delete_doc(&mut self, id: Uuid) -> bool {
        let record = match self.get_doc(id) {
            Some(r) => r,
            None => return false,
        };
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        let __bytes = serde_json::to_vec(&record).unwrap_or_default();
        self.buffer
            .push((
                "Doc",
                forgedb_changefeed::ChangeKind::Deleted,
                __id_bytes.clone(),
                __bytes.clone(),
                true,
            ));
        self.pending_events
            .push(("Doc", forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __bytes));
        true
    }
    pub fn get_doc(&self, id: Uuid) -> Option<Doc> {
        let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
        for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
            if __ib == &__id_bytes {
                if *__del {
                    return None;
                }
                return serde_json::from_slice(__bytes).ok();
            }
        }
        let __db = self.inner.read().unwrap();
        __db.doc.get_at(&self.snap.doc, id)
    }
    pub fn all_doc(&self) -> Vec<Doc> {
        let __db = self.inner.read().unwrap();
        let mut __rows = __db.doc.all_at(&self.snap.doc);
        drop(__db);
        for (_, _kind, __ib, __bytes, __del) in &self.buffer {
            let __id_bytes_ref: &Vec<u8> = __ib;
            if *__del {
                __rows
                    .retain(|r| {
                        serde_json::to_vec(&r.id).unwrap_or_default().as_slice()
                            != __id_bytes_ref.as_slice()
                    });
            } else if let Ok(__rec) = serde_json::from_slice::<Doc>(__bytes) {
                let __staged_id = __rec.id;
                if let Some(__pos) = __rows.iter().position(|r| r.id == __staged_id) {
                    __rows[__pos] = __rec;
                } else {
                    __rows.push(__rec);
                }
            }
        }
        __rows
    }
}
impl Database {
    pub fn shared(self) -> SharedDatabase {
        let seq = std::sync::Arc::clone(&self.seq);
        SharedDatabase {
            inner: std::sync::Arc::new(std::sync::RwLock::new(self)),
            seq,
        }
    }
    pub(crate) fn __apply_and_commit_concurrent_buffer(
        &mut self,
        buffer: Vec<
            (&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool),
        >,
        events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>)>,
    ) -> Result<Vec<(Vec<u8>, u64)>, TxError> {
        let mut __marks: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
        let mut __wal_marks: std::collections::BTreeMap<&'static str, u64> = std::collections::BTreeMap::new();
        for (__model_tag, _kind, _id_bytes, _bytes, _deleted) in &buffer {
            match *__model_tag {
                "User" => {
                    if !__marks.contains_key("User") {
                        let __rc = self.user.row_count;
                        let __wb = self.user.wal.size().unwrap_or(0);
                        __marks.insert("User", __rc);
                        __wal_marks.insert("User", __wb);
                        self.user.in_transaction = true;
                    }
                }
                "Post" => {
                    if !__marks.contains_key("Post") {
                        let __rc = self.post.row_count;
                        let __wb = self.post.wal.size().unwrap_or(0);
                        __marks.insert("Post", __rc);
                        __wal_marks.insert("Post", __wb);
                        self.post.in_transaction = true;
                    }
                }
                "Tag" => {
                    if !__marks.contains_key("Tag") {
                        let __rc = self.tag.row_count;
                        let __wb = self.tag.wal.size().unwrap_or(0);
                        __marks.insert("Tag", __rc);
                        __wal_marks.insert("Tag", __wb);
                        self.tag.in_transaction = true;
                    }
                }
                "Metric" => {
                    if !__marks.contains_key("Metric") {
                        let __rc = self.metric.row_count;
                        let __wb = self.metric.wal.size().unwrap_or(0);
                        __marks.insert("Metric", __rc);
                        __wal_marks.insert("Metric", __wb);
                        self.metric.in_transaction = true;
                    }
                }
                "Doc" => {
                    if !__marks.contains_key("Doc") {
                        let __rc = self.doc.row_count;
                        let __wb = self.doc.wal.size().unwrap_or(0);
                        __marks.insert("Doc", __rc);
                        __wal_marks.insert("Doc", __wb);
                        self.doc.in_transaction = true;
                    }
                }
                _ => {}
            }
        }
        let mut __staged_events: Vec<
            (&'static str, forgedb_changefeed::ChangeKind, usize, Vec<u8>),
        > = Vec::new();
        for (__model_tag, __kind, _id_bytes, __bytes, __deleted) in &buffer {
            let __result: Result<(), TxError> = (|| {
                match *__model_tag {
                    "User" => {
                        let __record: User = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.user.__stage_append(__record, *__deleted);
                        __staged_events
                            .push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                    "Post" => {
                        let __record: Post = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.post.__stage_append(__record, *__deleted);
                        __staged_events
                            .push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                    "Tag" => {
                        let __record: Tag = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.tag.__stage_append(__record, *__deleted);
                        __staged_events
                            .push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                    "Metric" => {
                        let __record: Metric = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.metric.__stage_append(__record, *__deleted);
                        __staged_events
                            .push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                    "Doc" => {
                        let __record: Doc = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.doc.__stage_append(__record, *__deleted);
                        __staged_events
                            .push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(e) = __result {
                let __touched: Vec<&'static str> = __marks.keys().copied().collect();
                for __tag in &__touched {
                    match *__tag {
                        "User" => {
                            if let Some(&__mark) = __marks.get("User") {
                                let _ = self.user.__truncate_all_to(__mark);
                                self.user.row_count = __mark;
                                if let Some(&__wm) = __wal_marks.get("User") {
                                    let _ = self.user.wal.truncate_to(__wm);
                                }
                                self.user.in_transaction = false;
                                self.user.run_deferred_maintenance();
                            }
                        }
                        "Post" => {
                            if let Some(&__mark) = __marks.get("Post") {
                                let _ = self.post.__truncate_all_to(__mark);
                                self.post.row_count = __mark;
                                if let Some(&__wm) = __wal_marks.get("Post") {
                                    let _ = self.post.wal.truncate_to(__wm);
                                }
                                self.post.in_transaction = false;
                                self.post.run_deferred_maintenance();
                            }
                        }
                        "Tag" => {
                            if let Some(&__mark) = __marks.get("Tag") {
                                let _ = self.tag.__truncate_all_to(__mark);
                                self.tag.row_count = __mark;
                                if let Some(&__wm) = __wal_marks.get("Tag") {
                                    let _ = self.tag.wal.truncate_to(__wm);
                                }
                                self.tag.in_transaction = false;
                                self.tag.run_deferred_maintenance();
                            }
                        }
                        "Metric" => {
                            if let Some(&__mark) = __marks.get("Metric") {
                                let _ = self.metric.__truncate_all_to(__mark);
                                self.metric.row_count = __mark;
                                if let Some(&__wm) = __wal_marks.get("Metric") {
                                    let _ = self.metric.wal.truncate_to(__wm);
                                }
                                self.metric.in_transaction = false;
                                self.metric.run_deferred_maintenance();
                            }
                        }
                        "Doc" => {
                            if let Some(&__mark) = __marks.get("Doc") {
                                let _ = self.doc.__truncate_all_to(__mark);
                                self.doc.row_count = __mark;
                                if let Some(&__wm) = __wal_marks.get("Doc") {
                                    let _ = self.doc.wal.truncate_to(__wm);
                                }
                                self.doc.in_transaction = false;
                                self.doc.run_deferred_maintenance();
                            }
                        }
                        _ => {}
                    }
                }
                return Err(e);
            }
        }
        let mut __journal: Vec<(String, u64)> = Vec::new();
        let __touched: Vec<&'static str> = __marks.keys().copied().collect();
        for __tag in &__touched {
            match *__tag {
                "User" => {
                    let _ = self.user.commit();
                    let _ = self.user.wal.flush();
                    __journal.push(("User".to_string(), self.user.row_count as u64));
                }
                "Post" => {
                    let _ = self.post.commit();
                    let _ = self.post.wal.flush();
                    __journal.push(("Post".to_string(), self.post.row_count as u64));
                }
                "Tag" => {
                    let _ = self.tag.commit();
                    let _ = self.tag.wal.flush();
                    __journal.push(("Tag".to_string(), self.tag.row_count as u64));
                }
                "Metric" => {
                    let _ = self.metric.commit();
                    let _ = self.metric.wal.flush();
                    __journal.push(("Metric".to_string(), self.metric.row_count as u64));
                }
                "Doc" => {
                    let _ = self.doc.commit();
                    let _ = self.doc.wal.flush();
                    __journal.push(("Doc".to_string(), self.doc.row_count as u64));
                }
                _ => {}
            }
        }
        if let Some(__jrnl) = self._txn_journal.as_mut() {
            let __payload = serde_json::to_vec(&__journal)
                .map_err(|e| TxError::Io(e.to_string()))?;
            __jrnl
                .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                .map_err(|e| TxError::Io(e.to_string()))?;
            __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
        }
        for __tag in &__touched {
            match *__tag {
                "User" => {
                    self.user.__reindex_committed();
                    self.user.in_transaction = false;
                    self.user.run_deferred_maintenance();
                }
                "Post" => {
                    self.post.__reindex_committed();
                    self.post.in_transaction = false;
                    self.post.run_deferred_maintenance();
                }
                "Tag" => {
                    self.tag.__reindex_committed();
                    self.tag.in_transaction = false;
                    self.tag.run_deferred_maintenance();
                }
                "Metric" => {
                    self.metric.__reindex_committed();
                    self.metric.in_transaction = false;
                    self.metric.run_deferred_maintenance();
                }
                "Doc" => {
                    self.doc.__reindex_committed();
                    self.doc.in_transaction = false;
                    self.doc.run_deferred_maintenance();
                }
                _ => {}
            }
        }
        let mut __row_index_pairs: Vec<(Vec<u8>, u64)> = Vec::with_capacity(
            __staged_events.len(),
        );
        for (__model, __kind, __row, __bytes) in __staged_events {
            __row_index_pairs.push((__model.as_bytes().to_vec(), __row as u64));
            self.changefeed.emit(__model, __row, __kind);
            if let Some(__broker) = &self.broker {
                if let Ok(mut __b) = __broker.lock() {
                    let _ = __b.record(__model, __row as u64, __kind, __bytes);
                }
            }
        }
        let _ = events;
        Ok(__row_index_pairs)
    }
}
impl DatabaseReader {
    pub fn post_tags_at(&self, snap: &DatabaseSnapshot, id: Uuid) -> Vec<Tag> {
        self.post_tag_link
            .pairs_at(&snap.post_tag_link)
            .into_iter()
            .filter(|(left, _)| *left == id)
            .filter_map(|(_, right)| { self.tag.get_at(&snap.tag, right) })
            .collect()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostWithRelations {
    pub post: Post,
    pub author: Option<User>,
}
impl Database {
    pub fn post_with_relations(&self, id: Uuid) -> Option<PostWithRelations> {
        let post = self.post.get(id)?;
        let author = self.user.get(post.author);
        Some(PostWithRelations { post, author })
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub struct CoordinatedDatabase {
    shared: SharedDatabase,
    coordinator: std::sync::Arc<forgedb_coordinator::client::CoordinatorClient>,
    last_refreshed_lsn: std::sync::atomic::AtomicU64,
}
#[cfg(not(target_arch = "wasm32"))]
impl CoordinatedDatabase {
    pub fn transaction_coordinated<T>(
        &self,
        retries: u32,
        f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        use forgedb_changefeed::ChangeKind as __CK;
        use std::sync::atomic::Ordering;
        let __coord = std::sync::Arc::clone(&self.coordinator);
        let __seq_arc = std::sync::Arc::clone(&self.shared.seq);
        let __inner = std::sync::Arc::clone(&self.shared.inner);
        let mut __last_lsn = __coord.last_known_lsn();
        for __attempt in 0..=retries {
            {
                let __coord_lsn = __coord.last_known_lsn();
                let __refreshed = self.last_refreshed_lsn.load(Ordering::Acquire);
                if __coord_lsn > __refreshed {
                    let mut __db = __inner.write().unwrap();
                    __db.__peer_refresh();
                    self.last_refreshed_lsn.store(__coord_lsn, Ordering::Release);
                }
            }
            let __snap_lsn = {
                let mut __seq = __seq_arc.lock().unwrap();
                __seq.register_snapshot()
            };
            let __snap = {
                let __db = __inner.read().unwrap();
                __db.snapshot()
            };
            let mut __tx = ConcurrentTxHandle {
                inner: std::sync::Arc::clone(&__inner),
                snap: __snap,
                buffer: Vec::new(),
                staged_unique_keys: std::collections::BTreeSet::new(),
                pending_events: Vec::new(),
            };
            let __out = f(&mut __tx);
            let __val = match __out {
                Ok(v) => v,
                Err(e) => {
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    return Err(e);
                }
            };
            let mut __ws_keys: Vec<Vec<u8>> = Vec::new();
            for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                __ws_keys
                    .push(__forgedb_ws_key(&[b"r", __model.as_bytes(), __id_bytes]));
            }
            for (__mtag, __fname, __ekey) in &__tx.staged_unique_keys {
                __ws_keys
                    .push(
                        __forgedb_ws_key(
                            &[
                                b"u",
                                __mtag.as_bytes(),
                                __fname.as_bytes(),
                                __ekey.as_bytes(),
                            ],
                        ),
                    );
            }
            let __turn = {
                let mut __busy_retries = 0u32;
                loop {
                    match __coord.request_turn(__ws_keys.clone(), __last_lsn) {
                        Ok(grant) => break Ok(grant),
                        Err(
                            forgedb_coordinator::client::ClientError::Conflict {
                                conflict_key,
                            },
                        ) => {
                            break Err(forgedb_txn::CommitOutcome::Conflict {
                                key: conflict_key.into_boxed_slice(),
                            });
                        }
                        Err(forgedb_coordinator::client::ClientError::Busy) => {
                            if __busy_retries >= 5 {
                                break Err(forgedb_txn::CommitOutcome::Conflict {
                                    key: b"__busy__".to_vec().into_boxed_slice(),
                                });
                            }
                            __busy_retries += 1;
                            std::thread::sleep(
                                std::time::Duration::from_millis(20 * __busy_retries as u64),
                            );
                        }
                        Err(e) => {
                            let _ = __coord.reconnect();
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.release_snapshot(__snap_lsn);
                            return Err(TxError::Io(e.to_string()));
                        }
                    }
                }
            };
            match __turn {
                Err(_) => {
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    __last_lsn = __coord.last_known_lsn();
                    continue;
                }
                Ok((__turn_id, _reserved_lsn)) => {
                    let __kinds_copy: Vec<u8> = __tx
                        .buffer
                        .iter()
                        .map(|(_, k, _, _, _)| k.to_byte())
                        .collect();
                    let __bytes_copy: Vec<Vec<u8>> = __tx
                        .buffer
                        .iter()
                        .map(|(_, _, _, b, _)| b.clone())
                        .collect();
                    let __apply_result = {
                        let mut __db = __inner.write().unwrap();
                        __db.__apply_and_commit_concurrent_buffer(
                            __tx.buffer,
                            __tx.pending_events,
                        )
                    };
                    let (__model_tags, __row_indices) = match &__apply_result {
                        Ok(__pairs) => {
                            let tags: Vec<Vec<u8>> = __pairs
                                .iter()
                                .map(|(t, _)| t.clone())
                                .collect();
                            let rows: Vec<u64> = __pairs
                                .iter()
                                .map(|(_, r)| *r)
                                .collect();
                            (tags, rows)
                        }
                        Err(_) => (Vec::new(), Vec::new()),
                    };
                    let __ack = __coord
                        .committed(
                            __turn_id,
                            __model_tags,
                            __row_indices,
                            __kinds_copy,
                            __bytes_copy,
                        );
                    match __ack {
                        Ok(__lsn) => {
                            __last_lsn = __lsn;
                        }
                        Err(e) => {
                            eprintln!("coordinator: Committed ack error: {e}");
                            let _ = __coord.reconnect();
                        }
                    }
                    let mut __seq = __seq_arc.lock().unwrap();
                    __seq.release_snapshot(__snap_lsn);
                    return __apply_result.map(|_| __val);
                }
            }
        }
        Err(TxError::Conflict)
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Database {
    pub(crate) fn __peer_refresh(&mut self) {
        let __from = self.user.row_count;
        self.user.__sync_columns_from_disk();
        self.user.__reindex_delta(__from);
        let __from = self.post.row_count;
        self.post.__sync_columns_from_disk();
        self.post.__reindex_delta(__from);
        let __from = self.tag.row_count;
        self.tag.__sync_columns_from_disk();
        self.tag.__reindex_delta(__from);
        let __from = self.metric.row_count;
        self.metric.__sync_columns_from_disk();
        self.metric.__reindex_delta(__from);
        let __from = self.doc.row_count;
        self.doc.__sync_columns_from_disk();
        self.doc.__reindex_delta(__from);
    }
    pub fn connect(
        root: std::path::PathBuf,
        socket_path: std::path::PathBuf,
    ) -> Result<CoordinatedDatabase, TxError> {
        let coordinator = forgedb_coordinator::client::CoordinatorClient::connect(
                &socket_path,
            )
            .map_err(|e| TxError::CoordinatorUnavailable(e.to_string()))?;
        let shared = Self::__open_with_lock(root, None).shared();
        Ok(CoordinatedDatabase {
            shared,
            coordinator: std::sync::Arc::new(coordinator),
            last_refreshed_lsn: std::sync::atomic::AtomicU64::new(0),
        })
    }
}
