An in-process, best-effort, bounded broadcast feed of [`ChangeEvent`]s.

Backed by [`tokio::sync::broadcast`]: any number of independent subscribers,
each with its own bounded buffer. A subscriber that lags past the buffer loses
the oldest events rather than blocking a writer; the feed never applies
backpressure to the write path. Cloning a `ChangeFeed` shares the same
channel, so a clone handed to a per-model storage publishes to the same
subscribers as the original. `Default` is [`ChangeFeed::new`] with a capacity
of 1024.
