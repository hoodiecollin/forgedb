The whole `value_size`-wide slot as an owned `Vec<u8>`: [`BufferedFixedColumn::read_slice`] plus a copy. Returns `InvalidInput` if `slot >= len()`.
