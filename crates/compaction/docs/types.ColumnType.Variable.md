A `variable/<column>_data.bin` payload file paired with `<column>_offsets.bin`, which holds
one little-endian `(offset: u64, length: u64)` pair per row.
