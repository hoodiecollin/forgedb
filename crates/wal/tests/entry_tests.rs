use forgedb_wal::*;

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
    // A payload consisting entirely of 0x00 bytes must round-trip byte-identical.
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
    // Build a valid Raw entry, then strip bytes from the end — the truncated
    // form must be rejected, not silently accepted with a partial payload.
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

    // Flip a bit in the middle of the entry body (after the 4-byte length
    // prefix) to corrupt the checksum-covered region.
    bytes[10] ^= 0xFF;

    let result = WalEntry::from_bytes(&bytes);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_unknown_op_type_byte_is_rejected() {
    // Craft an entry whose operation type byte is not 0x20 (Raw). The parser
    // must reject it rather than silently producing garbage.
    //
    // Build a valid Raw entry, then overwrite the op-type byte (byte index 4,
    // right after the 4-byte length prefix) with an unknown value. The CRC
    // will no longer match, so this also verifies CRC rejection — which is the
    // correct behaviour: a corrupt type byte IS a corrupt entry.
    let entry = WalEntry::raw("AnyModel", b"payload".to_vec());
    let mut bytes = entry.to_bytes();
    // Byte 4 is the op-type byte (after the 4-byte total_length field).
    bytes[4] = 0x01; // was a structured Insert byte in the old API; now unknown
    let result = WalEntry::from_bytes(&bytes);
    assert!(result.is_err());
}
