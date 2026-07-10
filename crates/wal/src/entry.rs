//! WAL Entry types and serialization

/// WAL operation types.
///
/// Only the `Raw` variant remains. The WAL stores opaque bytes and never
/// interprets their content. The (generated) caller owns the encoding; the WAL
/// provides framing, CRC integrity, and torn-tail crash safety.
#[derive(Debug, Clone, PartialEq)]
pub enum WalOperation {
    /// Opaque, schema-agnostic payload. The WAL stores and returns these bytes
    /// verbatim and never interprets them — the (generated) caller owns the
    /// encoding. This is the identity-preserving write path.
    Raw { payload: Vec<u8> },
}

impl WalOperation {
    /// Get the operation type byte used in the on-disk framing.
    pub fn type_byte(&self) -> u8 {
        match self {
            WalOperation::Raw { .. } => 0x20,
        }
    }

    /// Serialize the operation payload to bytes (excluding the type byte and
    /// model-name framing, which are written by [`WalEntry::to_bytes`]).
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            WalOperation::Raw { payload } => {
                // 4-byte little-endian length prefix so `from_bytes` can
                // reconstruct the exact slice even if the entry data section
                // grows in a future format revision.
                let mut bytes = Vec::with_capacity(4 + payload.len());
                bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                bytes.extend_from_slice(payload);
                bytes
            }
        }
    }

    /// Deserialize an operation from `(type_byte, payload_bytes)`.
    pub fn from_bytes(type_byte: u8, bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};

        match type_byte {
            0x20 => {
                if bytes.len() < 4 {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Raw payload length",
                    ));
                }
                let payload_len =
                    u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
                if bytes.len() < 4 + payload_len {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "Incomplete Raw payload data",
                    ));
                }
                let payload = bytes[4..4 + payload_len].to_vec();
                Ok(WalOperation::Raw { payload })
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unknown operation type: 0x{:02x}", type_byte),
            )),
        }
    }
}

/// A complete WAL entry: a model-name routing tag plus an operation.
///
/// The `model_name` field is an opaque string stored verbatim in the entry
/// header. The WAL never interprets it. Generated code uses it to route
/// replayed entries back to the correct model; ForgeDB itself never branches
/// on it.
#[derive(Debug, Clone, PartialEq)]
pub struct WalEntry {
    pub model_name: String,
    pub operation: WalOperation,
}

impl WalEntry {
    /// Create a raw opaque-bytes entry.
    ///
    /// `model_name` is an opaque routing/debug tag stored verbatim in the
    /// entry header. The WAL never interprets it. The generated caller uses it
    /// to identify which model a replayed entry belongs to.
    ///
    /// `payload` is stored verbatim and returned byte-identical on replay. The
    /// WAL neither inspects nor encodes the payload — the caller owns the
    /// encoding entirely.
    pub fn raw(model_name: impl Into<String>, payload: Vec<u8>) -> Self {
        WalEntry {
            model_name: model_name.into(),
            operation: WalOperation::Raw { payload },
        }
    }

    /// Serialize the entry to bytes including the CRC32 checksum.
    ///
    /// Wire format (see `lib.rs` for the full layout comment):
    /// `[4: total_length][1: op_type][2: model_name_len][N: model_name][M: op_data][4: crc32]`
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Operation type byte
        bytes.push(self.operation.type_byte());

        // Model name
        let model_bytes = self.model_name.as_bytes();
        bytes.extend_from_slice(&(model_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(model_bytes);

        // Operation data
        bytes.extend_from_slice(&self.operation.to_bytes());

        // CRC32 over everything so far
        let checksum = crc32fast::hash(&bytes);

        // Prepend total length (everything after this 4-byte field)
        let total_length = bytes.len() + 4; // +4 for checksum
        let mut result = Vec::with_capacity(4 + total_length);
        result.extend_from_slice(&(total_length as u32).to_le_bytes());
        result.extend_from_slice(&bytes);
        result.extend_from_slice(&checksum.to_le_bytes());

        result
    }

    /// Deserialize an entry from a byte slice.
    ///
    /// Returns `(entry, bytes_consumed)` on success. Returns an error on
    /// truncation (torn tail) or CRC mismatch.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<(Self, usize)> {
        use std::io::{Error, ErrorKind};

        if bytes.len() < 4 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete entry length",
            ));
        }

        let total_length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

        // A valid entry always carries at least a 4-byte trailing checksum. A
        // corrupt length prefix reporting fewer than 4 bytes would otherwise
        // underflow `entry_bytes.len() - 4` below and panic.
        if total_length < 4 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Entry length too short to contain a checksum",
            ));
        }

        if bytes.len() < 4 + total_length {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete entry"));
        }

        let entry_bytes = &bytes[4..4 + total_length];

        // Verify CRC32
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

        // Parse inner fields
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

        Ok((WalEntry { model_name, operation }, 4 + total_length))
    }
}
