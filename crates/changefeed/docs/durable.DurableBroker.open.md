Open or create the log at `path`, creating parent directories as needed.

An existing log is scanned to recover the watermark and the earliest retained
offset, so offsets stay monotonic across restarts. An incomplete frame at the
end of the file (a torn write) is truncated away and the truncation synced; a
frame whose checksum or length prefix is invalid is an error and `open`
fails. `capacity` bounds each live subscriber's in-memory buffer (clamped to
at least 1): a subscriber that lags past it loses the oldest live events and
must fall back to durable replay.

A broker whose file did not exist reports [`DurableBroker::watermark`] `0`
and [`DurableBroker::earliest_retained`] `0` until its first record.
