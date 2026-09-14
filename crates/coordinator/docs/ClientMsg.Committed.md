Report that the granted turn's data-plane write is durable, and hand over the opaque payload for the replication log.

The coordinator releases the turn first (so the next waiting client can be granted), then appends one event per row to `<root>/_coordinator_replication.log` under the configured fsync policy, then replies [`ServerMsg::Ack`]. A `turn_id` that is not the current turn is answered with [`ServerMsg::Error`] and nothing is appended.
