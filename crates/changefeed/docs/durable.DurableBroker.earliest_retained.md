The lowest offset still present in the log.

A follower whose last applied offset is below `earliest_retained() - 1`
cannot resume by replay, because events it needs have been pruned; it must
re-baseline from a full copy of the data. When the log holds no events this
is `watermark() + 1`, except that a broker whose file did not exist at
[`DurableBroker::open`] reports `0` until its first record.
