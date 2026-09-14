Drop every event with `offset <= through`, rewriting the log to retain
only newer events. Advances [`earliest_retained`]; followers still below
the new earliest must re-baseline from a snapshot.

Retention policy (when to prune) is the caller's — typically after a base
snapshot has advanced past `through`, or on a size bound. Rewrites via a
temp file + rename so a crash mid-prune leaves the old log intact.

[`earliest_retained`]: DurableBroker::earliest_retained
