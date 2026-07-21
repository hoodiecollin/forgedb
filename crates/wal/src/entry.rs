/// WAL Entry types and serialization
use std::collections::HashMap;

/// A value that can be stored in the WAL
#[derive(Debug, Clone, PartialEq)]
pub enum WalValue {
    U64(u64),
    String(String),
    Bool(bool),
    F64(f64),
    Uuid(uuid::Uuid),
    OptionU64(Option<u64>),
    OptionString(Option<String>),
    OptionBool(Option<bool>),
    OptionF64(Option<f64>),
    OptionUuid(Option<uuid::Uuid>),
}

impl WalValue {
    /// Get the type byte for serialization
    pub fn type_byte(&self) -> u8 {
        match self {
            WalValue::U64(_) => 0x01,
            WalValue::String(_) => 0x02,
            WalValue::Bool(_) => 0x03,
            WalValue::F64(_) => 0x04,
            WalValue::Uuid(_) => 0x05,
            WalValue::OptionU64(_) => 0x11,
            WalValue::OptionString(_) => 0x12,
            WalValue::OptionBool(_) => 0x13,
            WalValue::OptionF64(_) => 0x14,
            WalValue::OptionUuid(_) => 0x15,
        }
    }

    /// Serialize the value to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.type_byte()];

        match self {
            WalValue::U64(v) => bytes.extend_from_slice(&v.to_le_bytes()),
            WalValue::String(s) => {
                let string_bytes = s.as_bytes();
                bytes.extend_from_slice(&(string_bytes.len() as u32).to_le_bytes());
                bytes.extend_from_slice(string_bytes);
            }
            WalValue::Bool(b) => bytes.push(if *b { 1 } else { 0 }),
            WalValue::F64(f) => bytes.extend_from_slice(&f.to_le_bytes()),
            WalValue::Uuid(u) => bytes.extend_from_slice(u.as_bytes()),
            WalValue::OptionU64(opt) => {
                bytes.push(if opt.is_some() { 1 } else { 0 });
                if let Some(v) = opt {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            WalValue::OptionString(opt) => {
                bytes.push(if opt.is_some() { 1 } else { 0 });
                if let Some(s) = opt {
                    let string_bytes = s.as_bytes();
                    bytes.extend_from_slice(&(string_bytes.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(string_bytes);
                }
            }
            WalValue::OptionBool(opt) => {
                bytes.push(if opt.is_some() { 1 } else { 0 });
                if let Some(b) = opt {
                    bytes.push(if *b { 1 } else { 0 });
                }
            }
            WalValue::OptionF64(opt) => {
                bytes.push(if opt.is_some() { 1 } else { 0 });
                if let Some(f) = opt {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
            }
            WalValue::OptionUuid(opt) => {
                bytes.push(if opt.is_some() { 1 } else { 0 });
                if let Some(u) = opt {
                    bytes.extend_from_slice(u.as_bytes());
                }
            }
        }

        bytes
    }

    /// Deserialize a value from bytes
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<(Self, usize)> {
        use std::io::{Error, ErrorKind};

        if bytes.is_empty() {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Empty value bytes"));
        }

        let type_byte = bytes[0];
        let mut offset = 1;

        let value = match type_byte {
            0x01 => {
                // U64
                if bytes.len() < offset + 8 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete U64"));
                }
                let val = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;
                WalValue::U64(val)
            }
            0x02 => {
                // String
                if bytes.len() < offset + 4 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete string length",
                    ));
                }
                let len =
                    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
                offset += 4;

                if bytes.len() < offset + len {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete string data",
                    ));
                }
                let s = String::from_utf8(bytes[offset..offset + len].to_vec())
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                offset += len;
                WalValue::String(s)
            }
            0x03 => {
                // Bool
                if bytes.len() < offset + 1 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete Bool"));
                }
                let val = bytes[offset] != 0;
                offset += 1;
                WalValue::Bool(val)
            }
            0x04 => {
                // F64
                if bytes.len() < offset + 8 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete F64"));
                }
                let val = f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;
                WalValue::F64(val)
            }
            0x05 => {
                // Uuid
                if bytes.len() < offset + 16 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete Uuid"));
                }
                let val = uuid::Uuid::from_slice(&bytes[offset..offset + 16])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                offset += 16;
                WalValue::Uuid(val)
            }
            0x11 => {
                // Option<U64>
                if bytes.len() < offset + 1 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Option<U64>",
                    ));
                }
                let is_some = bytes[offset] != 0;
                offset += 1;

                if is_some {
                    if bytes.len() < offset + 8 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<U64> value",
                        ));
                    }
                    let val = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                    offset += 8;
                    WalValue::OptionU64(Some(val))
                } else {
                    WalValue::OptionU64(None)
                }
            }
            0x12 => {
                // Option<String>
                if bytes.len() < offset + 1 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Option<String>",
                    ));
                }
                let is_some = bytes[offset] != 0;
                offset += 1;

                if is_some {
                    if bytes.len() < offset + 4 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<String> length",
                        ));
                    }
                    let len =
                        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
                    offset += 4;

                    if bytes.len() < offset + len {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<String> data",
                        ));
                    }
                    let s = String::from_utf8(bytes[offset..offset + len].to_vec())
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    offset += len;
                    WalValue::OptionString(Some(s))
                } else {
                    WalValue::OptionString(None)
                }
            }
            0x13 => {
                // Option<Bool>
                if bytes.len() < offset + 1 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Option<Bool>",
                    ));
                }
                let is_some = bytes[offset] != 0;
                offset += 1;

                if is_some {
                    if bytes.len() < offset + 1 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<Bool> value",
                        ));
                    }
                    let val = bytes[offset] != 0;
                    offset += 1;
                    WalValue::OptionBool(Some(val))
                } else {
                    WalValue::OptionBool(None)
                }
            }
            0x14 => {
                // Option<F64>
                if bytes.len() < offset + 1 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Option<F64>",
                    ));
                }
                let is_some = bytes[offset] != 0;
                offset += 1;

                if is_some {
                    if bytes.len() < offset + 8 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<F64> value",
                        ));
                    }
                    let val = f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                    offset += 8;
                    WalValue::OptionF64(Some(val))
                } else {
                    WalValue::OptionF64(None)
                }
            }
            0x15 => {
                // Option<Uuid>
                if bytes.len() < offset + 1 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Option<Uuid>",
                    ));
                }
                let is_some = bytes[offset] != 0;
                offset += 1;

                if is_some {
                    if bytes.len() < offset + 16 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete Option<Uuid> value",
                        ));
                    }
                    let val = uuid::Uuid::from_slice(&bytes[offset..offset + 16])
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    offset += 16;
                    WalValue::OptionUuid(Some(val))
                } else {
                    WalValue::OptionUuid(None)
                }
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unknown type byte: {}", type_byte),
                ))
            }
        };

        Ok((value, offset))
    }
}

/// Transaction ID type
pub type TransactionId = u64;

/// WAL operation types
#[derive(Debug, Clone, PartialEq)]
pub enum WalOperation {
    /// Insert a new record
    Insert {
        record_id: uuid::Uuid,
        fields: HashMap<String, WalValue>,
    },
    /// Update an existing record
    Update {
        record_id: uuid::Uuid,
        fields: HashMap<String, WalValue>,
    },
    /// Delete a record
    Delete { record_id: uuid::Uuid },
    /// Begin a transaction
    BeginTransaction { txn_id: TransactionId },
    /// Commit a transaction
    CommitTransaction { txn_id: TransactionId },
    /// Rollback a transaction
    RollbackTransaction { txn_id: TransactionId },
}

impl WalOperation {
    /// Get the operation type byte
    pub fn type_byte(&self) -> u8 {
        match self {
            WalOperation::Insert { .. } => 0x01,
            WalOperation::Update { .. } => 0x02,
            WalOperation::Delete { .. } => 0x03,
            WalOperation::BeginTransaction { .. } => 0x10,
            WalOperation::CommitTransaction { .. } => 0x11,
            WalOperation::RollbackTransaction { .. } => 0x12,
        }
    }

    /// Serialize the operation to bytes (without model name)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            WalOperation::Insert { record_id, fields } => {
                bytes.extend_from_slice(record_id.as_bytes());
                bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());

                for (key, value) in fields {
                    let key_bytes = key.as_bytes();
                    bytes.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
                    bytes.extend_from_slice(key_bytes);
                    bytes.extend_from_slice(&value.to_bytes());
                }
            }
            WalOperation::Update { record_id, fields } => {
                bytes.extend_from_slice(record_id.as_bytes());
                bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());

                for (key, value) in fields {
                    let key_bytes = key.as_bytes();
                    bytes.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
                    bytes.extend_from_slice(key_bytes);
                    bytes.extend_from_slice(&value.to_bytes());
                }
            }
            WalOperation::Delete { record_id } => {
                bytes.extend_from_slice(record_id.as_bytes());
            }
            WalOperation::BeginTransaction { txn_id } => {
                bytes.extend_from_slice(&txn_id.to_le_bytes());
            }
            WalOperation::CommitTransaction { txn_id } => {
                bytes.extend_from_slice(&txn_id.to_le_bytes());
            }
            WalOperation::RollbackTransaction { txn_id } => {
                bytes.extend_from_slice(&txn_id.to_le_bytes());
            }
        }

        bytes
    }

    /// Deserialize operation from bytes
    pub fn from_bytes(type_byte: u8, bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};

        match type_byte {
            0x01 | 0x02 => {
                // Insert or Update
                if bytes.len() < 16 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete record_id"));
                }

                let record_id = uuid::Uuid::from_slice(&bytes[0..16])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

                let mut offset = 16;

                if bytes.len() < offset + 4 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete field count",
                    ));
                }

                let field_count = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let mut fields = HashMap::new();

                for _ in 0..field_count {
                    if bytes.len() < offset + 2 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete field key length",
                        ));
                    }

                    let key_len =
                        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
                    offset += 2;

                    if bytes.len() < offset + key_len {
                        return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete field key"));
                    }

                    let key = String::from_utf8(bytes[offset..offset + key_len].to_vec())
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    offset += key_len;

                    let (value, value_len) = WalValue::from_bytes(&bytes[offset..])?;
                    offset += value_len;

                    fields.insert(key, value);
                }

                if type_byte == 0x01 {
                    Ok(WalOperation::Insert { record_id, fields })
                } else {
                    Ok(WalOperation::Update { record_id, fields })
                }
            }
            0x03 => {
                // Delete
                if bytes.len() < 16 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete record_id"));
                }

                let record_id = uuid::Uuid::from_slice(&bytes[0..16])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

                Ok(WalOperation::Delete { record_id })
            }
            0x10 => {
                // BeginTransaction
                if bytes.len() < 8 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete txn_id"));
                }

                let txn_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
                Ok(WalOperation::BeginTransaction { txn_id })
            }
            0x11 => {
                // CommitTransaction
                if bytes.len() < 8 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete txn_id"));
                }

                let txn_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
                Ok(WalOperation::CommitTransaction { txn_id })
            }
            0x12 => {
                // RollbackTransaction
                if bytes.len() < 8 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete txn_id"));
                }

                let txn_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
                Ok(WalOperation::RollbackTransaction { txn_id })
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unknown operation type: {}", type_byte),
            )),
        }
    }
}

/// A complete WAL entry
#[derive(Debug, Clone, PartialEq)]
pub struct WalEntry {
    pub model_name: String,
    pub operation: WalOperation,
}

impl WalEntry {
    /// Create an insert entry
    pub fn insert(
        model_name: String,
        record_id: uuid::Uuid,
        fields: HashMap<String, WalValue>,
    ) -> Self {
        WalEntry {
            model_name,
            operation: WalOperation::Insert { record_id, fields },
        }
    }

    /// Create an update entry
    pub fn update(
        model_name: String,
        record_id: uuid::Uuid,
        fields: HashMap<String, WalValue>,
    ) -> Self {
        WalEntry {
            model_name,
            operation: WalOperation::Update { record_id, fields },
        }
    }

    /// Create a delete entry
    pub fn delete(model_name: String, record_id: uuid::Uuid) -> Self {
        WalEntry {
            model_name,
            operation: WalOperation::Delete { record_id },
        }
    }

    /// Create a begin transaction entry
    pub fn begin_transaction(txn_id: TransactionId) -> Self {
        WalEntry {
            model_name: String::new(),
            operation: WalOperation::BeginTransaction { txn_id },
        }
    }

    /// Create a commit transaction entry
    pub fn commit_transaction(txn_id: TransactionId) -> Self {
        WalEntry {
            model_name: String::new(),
            operation: WalOperation::CommitTransaction { txn_id },
        }
    }

    /// Create a rollback transaction entry
    pub fn rollback_transaction(txn_id: TransactionId) -> Self {
        WalEntry {
            model_name: String::new(),
            operation: WalOperation::RollbackTransaction { txn_id },
        }
    }

    /// Serialize the entry to bytes with checksum
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Operation type
        bytes.push(self.operation.type_byte());

        // Model name
        let model_bytes = self.model_name.as_bytes();
        bytes.extend_from_slice(&(model_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(model_bytes);

        // Operation data
        bytes.extend_from_slice(&self.operation.to_bytes());

        // Calculate checksum
        let checksum = crc32fast::hash(&bytes);

        // Add total length at beginning
        let total_length = bytes.len() + 4; // +4 for checksum
        let mut result = Vec::new();
        result.extend_from_slice(&(total_length as u32).to_le_bytes());
        result.extend_from_slice(&bytes);
        result.extend_from_slice(&checksum.to_le_bytes());

        result
    }

    /// Deserialize an entry from bytes
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<(Self, usize)> {
        use std::io::{Error, ErrorKind};

        if bytes.len() < 4 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete entry length",
            ));
        }

        let total_length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

        if bytes.len() < 4 + total_length {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete entry"));
        }

        let entry_bytes = &bytes[4..4 + total_length];

        // Verify checksum
        let data_bytes = &entry_bytes[..entry_bytes.len() - 4];
        let stored_checksum =
            u32::from_le_bytes(entry_bytes[entry_bytes.len() - 4..].try_into().unwrap());
        let calculated_checksum = crc32fast::hash(data_bytes);

        if stored_checksum != calculated_checksum {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Checksum mismatch - entry corrupted",
            ));
        }

        // Parse entry
        let mut offset = 0;

        let op_type = data_bytes[offset];
        offset += 1;

        if data_bytes.len() < offset + 2 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete model name length",
            ));
        }

        let model_name_len =
            u16::from_le_bytes(data_bytes[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;

        if data_bytes.len() < offset + model_name_len {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete model name",
            ));
        }

        let model_name = String::from_utf8(data_bytes[offset..offset + model_name_len].to_vec())
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        offset += model_name_len;

        let operation = WalOperation::from_bytes(op_type, &data_bytes[offset..])?;

        Ok((
            WalEntry {
                model_name,
                operation,
            },
            4 + total_length,
        ))
    }
}
