Create a feed whose per-subscriber buffer holds up to `capacity` events.

`capacity` bounds the lag a slow subscriber may accumulate before it starts
dropping the oldest events; it does not bound the number of subscribers.
