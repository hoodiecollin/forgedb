Read one length-prefixed JSON frame from `reader`, rejecting a payload longer than `max_frame` bytes.

Reads the 4-byte big-endian length, fails with `InvalidData` if it exceeds `max_frame` (nothing further is consumed), then reads exactly that many bytes and deserializes them; a JSON error is also `InvalidData`, and a short read is `UnexpectedEof`. The coordinator calls this with [`server::CoordConfig::max_frame`] on every client frame, which bounds the memory one `RequestTurn` or `Committed` can demand.
