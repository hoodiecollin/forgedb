`forgedb-changefeed` — a schema-agnostic, in-process change-feed broadcast
primitive.

This is **Class-1 substrate**: a peer of `forgedb-storage` / `forgedb-wal`
that knows nothing about any `.forge` schema. It carries only a *positional*
signal — "model `M` gained a row at index `N`" — and never decodes a record,
reads a field, or filters by field value. Those responsibilities live in
**generated code**: the generated `insert()` / `link_*` methods (which know
the model's identity and hold the typed record) emit into this feed, and a
generated per-model WebSocket handler turns a `(model, row_index)` signal
back into a typed payload and applies any generated per-model filter.

The append-only storage engine *is* a change log — every write is positionally
an event. This primitive simply fans that fact out to subscribers, best-effort
and in-process.

## Two feeds, one red line

- [`ChangeFeed`] (this module) — the **in-process, best-effort** signal:
  ephemeral `tokio::sync::broadcast` of a field-blind `{model, row_index,
  kind}`, no durability and no offsets. Directions A + B (#62).
- [`durable::DurableBroker`] — the **durable, offset-addressed, resumable**
  broker (#82, Direction C): the same field-blind signal *plus* the opaque
  committed row bytes and a monotonic global offset, persisted to a
  CRC-framed append-only log so a subscriber can reconnect and resume from a
  watermark. The substrate the WASM read-replica follower pulls from (#110).

Both stay field-blind. The offset is an **opaque ordering token**, not a
decoded value; the bytes are carried verbatim and never interpreted here.

## The red line

`ChangeEvent` holds a `&'static str` model name and a `usize` row index — no
field data, ever. If this crate ever needed to inspect a record's contents to
route or filter an event, it would have become the forbidden generic engine,
event-shaped. It does not: routing by model name and materialization of the
typed payload both happen in generated code.
