`forgedb-changefeed::durable` — a durable, offset-addressed, resumable change
broker (#82, realtime Direction C).

Where [`crate::ChangeFeed`] is an in-process, best-effort *signal*
([`tokio::sync::broadcast`] of `{model, row_index, kind}`, no durability),
[`DurableBroker`] is the substrate a cross-process / cross-network follower
resumes from. It records each committed change to a **CRC-framed,
append-only log** and assigns a **monotonic global offset**, so a subscriber
that persists the last offset it applied can reconnect and replay everything
after it — the resumable read-replica contract the WASM follower (#110)
needs.

## The red line holds — this crate still decodes nothing

A [`PersistedEvent`] carries `model` (an **opaque routing tag**, exactly like
`forgedb-wal`'s model-name header), `row_index`, `kind`, `offset`, and
`bytes` — the **opaque committed row bytes**, stored and returned verbatim.
There is no field-typed member and no per-model branch anywhere in this
module: routing by model name and materialization of the typed record both
stay in generated code, precisely as with the in-process feed. The broker
moves opaque bytes tagged by an opaque name; that is all it will ever do.

## The offset *is* the ordering contract

Offsets are a single monotonically increasing `u64` assigned in [`record`]
order. Because the generated server is a single writer, `record` order *is*
the commit order (the server-side `DatabaseSnapshot` boundary), so one global
offset sequences changes **across all models** without the broker ever
understanding a relation or a foreign key. A follower applies strictly in
offset order and is **idempotent by absolute offset** — replaying an already
applied offset is a no-op, the same discipline as WAL replay by absolute row
index. That idempotency is what makes "resume from the last persisted offset"
correct even if the last few in-flight events were re-sent.

[`record`]: DurableBroker::record
