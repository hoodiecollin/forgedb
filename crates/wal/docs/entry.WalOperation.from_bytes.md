Decode operation data that [`WalEntry::from_bytes`] has already separated from
the header.

`type_byte` selects the variant; `0x20` reads a little-endian `u32` length and
then that many payload bytes. Errors: `InvalidData` for an unknown type byte,
`UnexpectedEof` when `bytes` is shorter than the length field claims.
