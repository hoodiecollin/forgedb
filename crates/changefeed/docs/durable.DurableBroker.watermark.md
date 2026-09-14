The highest offset assigned so far, `0` if nothing has ever been recorded. A
follower that has applied through this offset is fully caught up.
[`DurableBroker::prune_through`] does not move it.
