#[derive(Debug, Clone, PartialEq)]
pub enum WalOperation {
    Raw { payload: Vec<u8> },
}

impl WalOperation {
    pub fn type_byte(&self) -> u8 {
        match self {
            WalOperation::Raw { .. } => 0x20,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            WalOperation::Raw { payload } => {
                let mut bytes = Vec::with_capacity(4 + payload.len());
                bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                bytes.extend_from_slice(payload);
                bytes
            }
        }
    }

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

#[derive(Debug, Clone, PartialEq)]
pub struct WalEntry {
    pub model_name: String,
    pub operation: WalOperation,
}

impl WalEntry {
    pub fn raw(model_name: impl Into<String>, payload: Vec<u8>) -> Self {
        WalEntry {
            model_name: model_name.into(),
            operation: WalOperation::Raw { payload },
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.push(self.operation.type_byte());

        let model_bytes = self.model_name.as_bytes();
        bytes.extend_from_slice(&(model_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(model_bytes);

        bytes.extend_from_slice(&self.operation.to_bytes());

        let checksum = crc32fast::hash(&bytes);

        let total_length = bytes.len() + 4;
        let mut result = Vec::with_capacity(4 + total_length);
        result.extend_from_slice(&(total_length as u32).to_le_bytes());
        result.extend_from_slice(&bytes);
        result.extend_from_slice(&checksum.to_le_bytes());

        result
    }

    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<(Self, usize)> {
        use std::io::{Error, ErrorKind};

        if bytes.len() < 4 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete entry length",
            ));
        }

        let total_length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

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
