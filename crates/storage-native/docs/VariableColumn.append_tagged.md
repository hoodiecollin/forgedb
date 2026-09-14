Append a single tag byte followed by `value`'s bytes, as one row (#231).

Exactly equivalent to `append_string(&format!("{tag_char}{value}"))` for any
`tag` that is a valid single-byte UTF-8 scalar, but without materializing
the concatenation: the tag and the value are written from their own slices
in one vectored write, so a tagged append costs no allocation and no copy
of the value.

This is the write-side sibling of [`BufferedVariableColumn::read_str`]
(#224) and it is schema-agnostic in exactly the same way — it knows nothing
about what the tag *means*. Generated code uses it for the 1-byte presence
tag on nullable `string`/`json` columns (`0x00` = None, `0x01` = Some), but
the column stores opaque bytes either way.

# Note on UTF-8

The stored row is only valid UTF-8 — and so only readable by
[`read_string`](VariableColumn::read_string) — when `tag < 0x80`. Tags at or
above `0x80` produce a leading continuation/lead byte with nothing to
complete it. The column itself is byte-oriented and will store them
faithfully; it is the caller's job to keep tags in the ASCII range if the
row must round-trip as a `String`.
