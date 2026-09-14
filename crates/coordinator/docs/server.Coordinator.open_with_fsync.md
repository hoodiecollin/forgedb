Like [`open`](Self::open) but with an explicit replication-log fsync policy
(#156 Option C). `CoordFsync::Always` (the `open` default) preserves the
pre-#156 durability; `Never`/`Periodic` trade replication-log durability
for commit throughput (no committed client data is ever at risk — see
[`CoordFsync`]).
