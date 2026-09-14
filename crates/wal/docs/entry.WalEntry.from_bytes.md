Decode one record from the front of `bytes`.

Returns the entry and the number of bytes it occupied, length field included,
so the caller can advance to the next record. Errors: `UnexpectedEof` when
`bytes` is shorter than the record claims (a torn tail); `InvalidData` when the
length field is too small to hold a checksum, the CRC32 does not match, the
model name is not UTF-8, or the type byte is unknown.
