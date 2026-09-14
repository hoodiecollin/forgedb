Whether the value is the empty string.

The empty string is a legal `InlineStr` — it is the [`Default`], which the
generated id field's `#[serde(default)]` needs — and is rejected as a
*key* at write time (#252 res 5), not here.
