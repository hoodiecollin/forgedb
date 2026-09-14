A `Copy` UTF-8 string of at most `BYTES` bytes, stored inline.

## Why it exists

Generated code passes a model's identity by value everywhere: reads, deletes, relation
resolution, index keys, change events. That is free for `Uuid` and the integer identities
because they are `Copy`; `String` is not. `InlineStr` is the key type a `string(N)` or
`string(N!)` identity compiles to, and so is any foreign key that points at one.

## What it enforces

Only the capacity bound. Construction goes through `TryFrom<&str>` (also `TryFrom<&String>`
and `FromStr`), which fails with [`InlineStrError`] when the text is longer than `BYTES`; the
capacity is a bound, never a truncation. The crate knows nothing about schemas, so the
character-set rule a string identity's value must obey is checked by the generated write path,
not here.

## Parameterized in bytes, declared in characters

`BYTES` is a byte capacity because the crate cannot compute `CHARS * 4` in a const generic
without unstable features; the generator emits the constant. For an identity the two agree
(`string(26)` and `string(26!)` both give `InlineStr<26>`) because a string identity is one
byte per character. `BYTES` must be at most `65535`, since the length is held in a `u16`.

## Trait behavior

Equality, ordering and hashing are defined on [`Self::as_str`]: bytes past the length are not
part of the value. `Serialize` and `Deserialize` use a plain JSON string, hand-written so that
every capacity works rather than only the array lengths serde derives for. `Debug` prints the
text, not the buffer. `Default` is the empty string. `Deref<Target = str>` and `AsRef<str>`
give access to the `str` API, and `Display` renders the text.
