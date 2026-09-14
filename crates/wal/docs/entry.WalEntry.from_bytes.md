Deserialize an entry from a byte slice.

Returns `(entry, bytes_consumed)` on success. Returns an error on
truncation (torn tail) or CRC mismatch.
