A durable, offset-addressed, resumable broker over one append-only log file.

[`DurableBroker::record`] appends a CRC-framed [`PersistedEvent`] to the log
and fans the same event out to live [`DurableBroker::subscribe`] receivers. A
follower resumes with [`DurableBroker::read_from`] from its last applied
offset, or with [`DurableBroker::catch_up_from`] to stitch that replay onto a
live subscription without a gap.

`record` takes `&mut self` and every read method takes `&self`, so the owner
of the broker serializes writes and offsets are a faithful global order. The
generated server keeps one behind an `Arc<Mutex<_>>` at
`<data dir>/_replication.log`; `forgedb coordinate` holds one for the writers
it coordinates.
