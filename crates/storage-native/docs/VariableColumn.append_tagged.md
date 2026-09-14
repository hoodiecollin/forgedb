Append a single tag byte followed by `value`'s bytes, as one row.

Equivalent to appending the concatenation as a string, but without materializing it: the tag and the value are written from their own slices in one vectored write, so a tagged append costs no allocation and no copy of the value. The column knows nothing about what the tag means; generated code uses it for the one-byte presence tag on nullable string and JSON columns, and the column stores opaque bytes either way.

The stored row is only valid UTF-8, and so only readable by [`VariableColumn::read_string`], when `tag < 0x80`. A higher tag byte is stored faithfully but leaves an incomplete UTF-8 sequence at the front of the row; keep tags in the ASCII range if the row must round-trip as a `String`.
