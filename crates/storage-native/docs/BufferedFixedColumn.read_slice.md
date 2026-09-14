The whole `value_size`-wide slot, borrowed from the buffer.

The zero-copy counterpart of [`BufferedFixedColumn::read_bytes`]. Borrowing is possible here because this type owns its bytes; [`FixedColumn`] and [`FixedColumnReader`] read through the file on every access and have nothing to lend. Generated code decodes the wider fixed layouts (raw byte fields, inline structs, fixed arrays, nullable and optional-FK encodings) from this slice.

Returns `InvalidInput` if `slot >= len()`.
