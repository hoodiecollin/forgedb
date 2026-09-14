Drop every event with `offset <= through` by rewriting the log to hold only
the newer ones, then advance [`DurableBroker::earliest_retained`].

The retained events are written to a sibling temporary file, synced, and
renamed over the log, so a crash mid-prune leaves the old log intact. The
watermark is unchanged; if nothing remains, `earliest_retained` becomes
`watermark() + 1`. When to prune is the caller's policy; a follower still
below the new earliest must re-baseline.
