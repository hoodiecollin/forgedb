use forgedb_wal::*;
use std::collections::HashMap;

#[test]
fn test_entry_from_bytes_rejects_short_length_prefix() {
    // A length prefix smaller than the mandatory 4-byte checksum must be
    // rejected with an error, not panic on an underflowing slice range.
    for total_length in 0u32..4 {
        let mut bytes = total_length.to_le_bytes().to_vec();
        bytes.extend(std::iter::repeat_n(0u8, total_length as usize));
        let result = WalEntry::from_bytes(&bytes);
        assert!(
            result.is_err(),
            "length prefix {total_length} should be rejected, not parsed"
        );
    }
}

#[test]
fn test_wal_value_u64() {
    let val = WalValue::U64(12345);
    let bytes = val.to_bytes();
    let (decoded, len) = WalValue::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, val);
    assert_eq!(len, bytes.len());
}

#[test]
fn test_wal_value_string() {
    let val = WalValue::String("hello world".to_string());
    let bytes = val.to_bytes();
    let (decoded, len) = WalValue::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, val);
    assert_eq!(len, bytes.len());
}

#[test]
fn test_wal_value_option_uuid() {
    let val = WalValue::OptionUuid(Some(uuid::Uuid::new_v4()));
    let bytes = val.to_bytes();
    let (decoded, len) = WalValue::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, val);
    assert_eq!(len, bytes.len());

    let val_none = WalValue::OptionUuid(None);
    let bytes = val_none.to_bytes();
    let (decoded, len) = WalValue::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, val_none);
    assert_eq!(len, bytes.len());
}

#[test]
fn test_wal_entry_insert() {
    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    fields.insert("age".to_string(), WalValue::U64(30));

    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);
    let bytes = entry.to_bytes();
    let (decoded, len) = WalEntry::from_bytes(&bytes).unwrap();

    assert_eq!(decoded, entry);
    assert_eq!(len, bytes.len());
}

#[test]
fn test_wal_entry_delete() {
    let record_id = uuid::Uuid::new_v4();
    let entry = WalEntry::delete("User".to_string(), record_id);
    let bytes = entry.to_bytes();
    let (decoded, len) = WalEntry::from_bytes(&bytes).unwrap();

    assert_eq!(decoded, entry);
    assert_eq!(len, bytes.len());
}

#[test]
fn test_wal_entry_transaction() {
    let txn_id = 42;
    let begin = WalEntry::begin_transaction(txn_id);
    let bytes = begin.to_bytes();
    let (decoded, _) = WalEntry::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, begin);

    let commit = WalEntry::commit_transaction(txn_id);
    let bytes = commit.to_bytes();
    let (decoded, _) = WalEntry::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, commit);

    let rollback = WalEntry::rollback_transaction(txn_id);
    let bytes = rollback.to_bytes();
    let (decoded, _) = WalEntry::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, rollback);
}

#[test]
fn test_corrupted_checksum() {
    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );

    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);
    let mut bytes = entry.to_bytes();

    // Corrupt a byte in the middle
    bytes[10] ^= 0xFF;

    let result = WalEntry::from_bytes(&bytes);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}
