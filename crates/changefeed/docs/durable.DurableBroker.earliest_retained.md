The lowest offset still retained in the durable log.

A follower whose last-applied offset is **below** `earliest_retained() - 1`
has fallen off the retained tail and must re-baseline from a full snapshot
(`forgedb-backup`) before resuming — this is the snapshot-vs-tail cutover
point. Equals [`watermark`](DurableBroker::watermark)` + 1` when the log
is empty.
