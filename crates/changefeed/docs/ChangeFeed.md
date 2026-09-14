An in-process, best-effort, bounded broadcast feed of [`ChangeEvent`]s.

Backed by [`tokio::sync::broadcast`]: any number of independent subscribers,
each with its own bounded ring buffer. A subscriber that lags past the buffer
drops the oldest events rather than blocking a writer — the feed is best-effort
and never applies backpressure to the insert path. Cloning a `ChangeFeed`
shares the same underlying channel, so a clone handed to a per-model storage
publishes to the same subscribers as the original.
