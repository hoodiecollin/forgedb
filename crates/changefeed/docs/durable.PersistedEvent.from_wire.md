Decode exactly one frame produced by [`PersistedEvent::to_wire`].

Returns an `InvalidData` error on a CRC mismatch, a length prefix too short to
hold a checksum, an unknown kind byte or a non-UTF-8 model name, and an
`UnexpectedEof` error when `buf` ends before the frame does or the payload's
own length fields overrun it. Bytes after the first complete frame are
ignored.
