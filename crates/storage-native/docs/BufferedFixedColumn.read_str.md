The whole `value_size`-wide slot at `slot` as UTF-8, **borrowed** (#238).

For a column where the slot *is* the value and carries no framing:
`string(N!)` (#238 res 6 — exactly N ASCII characters, therefore exactly N
bytes, therefore no length prefix), and any other fixed UTF-8 payload.

It deliberately does **not** understand a length prefix. Which slots carry
one, and how wide it is, is #238's per-declaration layout choice; teaching
it to a schema-agnostic crate would move generated knowledge into the
substrate. Prefixed slots read through [`Self::read_slice`] and decode in
generated code.

# Errors

`InvalidInput` if the slot is out of range; `InvalidData` if the slot's
bytes are not valid UTF-8 — same contract as
[`BufferedVariableColumn::read_str`].
