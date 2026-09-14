`forgedb-changefeed` — a schema-agnostic change-feed primitive: an in-process
broadcast signal plus a durable, offset-addressed broker.

Both halves are field-blind. They carry a model name, a row position and a
[`ChangeKind`], and the broker additionally carries the committed row bytes
verbatim; nothing here decodes a record, reads a field, or filters by value.
That work belongs to the generated code that links this crate: generated
`insert` / `update` / `delete` / `link` / `unlink` methods emit into the feed
and record into the broker, and generated handlers turn a `(model, row_index)`
signal back into a typed payload.

## Two feeds

- [`ChangeFeed`] — the **in-process, best-effort** signal: a
  [`tokio::sync::broadcast`] channel of [`ChangeEvent`] (`{model, row_index,
  kind}`), with no durability and no offsets. The generated server's per-model
  WebSocket subscriptions read from it.
- [`durable::DurableBroker`] — the **durable, offset-addressed, resumable**
  broker: the same signal plus the opaque committed row bytes and a monotonic
  global offset, appended to a CRC-framed log so a subscriber can persist the
  last offset it applied and resume from it. The generated server's
  `/replicate` WebSocket streams its frames, and `forgedb coordinate` holds
  one on behalf of the writers it coordinates.

The offset is an opaque ordering token, never a decoded value.
