Serialize the entry to bytes including the CRC32 checksum.

Wire format (see `lib.rs` for the full layout comment):
`[4: total_length][1: op_type][2: model_name_len][N: model_name][M: op_data][4: crc32]`
