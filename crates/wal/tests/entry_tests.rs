use forgedb_wal::*;

#[test]
fn test_entry_from_bytes_rejects_short_length_prefix() {
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
fn test_raw_entry_round_trip_typical_payload() {
    let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0x42];
    let entry = WalEntry::raw("Post", payload.clone());
    let bytes = entry.to_bytes();
    let (decoded, len) = WalEntry::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, entry);
    assert_eq!(len, bytes.len());
    match decoded.operation {
        WalOperation::Raw { payload: p } => assert_eq!(p, payload),
        #[allow(unreachable_patterns)]
        other => panic!("expected Raw, got {:?}", other),
    }
}

#[test]
fn test_raw_entry_round_trip_empty_payload() {
    let entry = WalEntry::raw("Empty", vec![]);
    let bytes = entry.to_bytes();
    let (decoded, len) = WalEntry::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, entry);
    assert_eq!(len, bytes.len());
    match decoded.operation {
        WalOperation::Raw { payload } => assert!(payload.is_empty()),
        #[allow(unreachable_patterns)]
        other => panic!("expected Raw, got {:?}", other),
    }
}

#[test]
fn test_raw_entry_round_trip_null_bytes_in_payload() {
    let payload = vec![0x00u8; 32];
    let entry = WalEntry::raw("NullBytes", payload.clone());
    let bytes = entry.to_bytes();
    let (decoded, _) = WalEntry::from_bytes(&bytes).unwrap();
    match decoded.operation {
        WalOperation::Raw { payload: p } => assert_eq!(p, payload),
        #[allow(unreachable_patterns)]
        other => panic!("expected Raw, got {:?}", other),
    }
}

#[test]
fn test_raw_entry_truncated_is_rejected() {
    let payload = b"some important row data".to_vec();
    let entry = WalEntry::raw("Model", payload);
    let bytes = entry.to_bytes();

    let truncated = &bytes[..bytes.len() - 1];
    let result = WalEntry::from_bytes(truncated);
    assert!(
        result.is_err(),
        "truncated Raw entry should be rejected, not parsed"
    );
}

#[test]
fn test_corrupted_checksum_is_rejected() {
    let payload = b"integrity check payload".to_vec();
    let entry = WalEntry::raw("Model", payload);
    let mut bytes = entry.to_bytes();

    bytes[10] ^= 0xFF;

    let result = WalEntry::from_bytes(&bytes);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_unknown_op_type_byte_is_rejected() {
    let entry = WalEntry::raw("AnyModel", b"payload".to_vec());
    let mut bytes = entry.to_bytes();
    bytes[4] = 0x01;
    let result = WalEntry::from_bytes(&bytes);
    assert!(result.is_err());
}
