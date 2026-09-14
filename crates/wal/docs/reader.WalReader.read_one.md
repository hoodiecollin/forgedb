Decode the record at the current cursor position and advance past it.

Returns `Ok(None)` when fewer than four bytes remain (end of file). Unlike
[`Self::read_all`], damage is reported: a length field larger than the rest of
the file is `InvalidData`, and a checksum or framing failure is the error from
[`WalEntry::from_bytes`].
