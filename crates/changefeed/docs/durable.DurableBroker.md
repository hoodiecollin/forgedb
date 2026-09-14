A durable, offset-addressed, resumable broker.

Records committed changes to an append-only, CRC-framed log and fans the same
events out to live [`subscribe`](DurableBroker::subscribe) receivers. A
follower catches up by [`read_from`](DurableBroker::read_from) its last
persisted offset, then attaches a live subscription — see
[`catch_up_from`](DurableBroker::catch_up_from) for the race-free stitch.

**Single-writer.** [`record`](DurableBroker::record) takes `&mut self`; the
generated server owns the one writer (v1 single-writer-per-process), so
`record` order is commit order and offsets are a faithful global ordering.
Read methods take `&self`.
