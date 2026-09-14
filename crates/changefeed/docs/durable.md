A durable, offset-addressed, resumable change broker.

Where [`ChangeFeed`] is an in-process, best-effort signal with no durability,
[`durable::DurableBroker`] is the substrate a cross-process or cross-network
follower resumes from. It appends each recorded change to a CRC-framed,
append-only log and assigns it a monotonic global offset, so a subscriber that
persists the last offset it applied can reconnect and replay everything after
it.

## Still field-blind

A [`durable::PersistedEvent`] carries `offset`, `model` (an opaque routing
tag), `row_index`, `kind` and `bytes` (the committed row bytes, stored and
returned verbatim). There is no field-typed member and no per-model branch
anywhere in this module; routing by model name and materializing the typed
record stay in generated code, as with the in-process feed.

## The offset is the ordering contract

Offsets are a single `u64` sequence starting at `1`, assigned in
[`durable::DurableBroker::record`] order. `record` takes `&mut self`, so
whoever owns the broker serializes writes through it, and the offset is a
total order across every model without the broker knowing a relation or a
foreign key. A follower applies strictly in offset order and treats an
already-applied offset as a no-op; that idempotency is what makes resuming
from the last persisted offset correct even when events near the boundary
are delivered twice.
