Release a snapshot.

Decrements the refcount for `s`; removes it from the map when the count
reaches zero so [`gc`] may prune entries older than the next-oldest snapshot.
