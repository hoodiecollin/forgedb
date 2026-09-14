Create a feed whose per-subscriber buffer holds up to `capacity` events.

`capacity` bounds the lag a slow subscriber may accumulate before it starts
losing the oldest events; it does not bound the number of subscribers. It
must be non-zero and at most `usize::MAX / 2`, or the underlying
[`tokio::sync::broadcast::channel`] panics.
