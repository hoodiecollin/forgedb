The whole `value_size`-wide slot as borrowed UTF-8.

For a column whose slot is the value with no framing, such as an exactly-N-character ASCII string. It does not understand a length prefix: which slots carry one, and how wide it is, is a per-declaration layout choice that belongs to generated code, which reads such slots through [`BufferedFixedColumn::read_slice`] and decodes them itself.

Returns `InvalidInput` if `slot >= len()` and `InvalidData` if the slot's bytes are not valid UTF-8.
