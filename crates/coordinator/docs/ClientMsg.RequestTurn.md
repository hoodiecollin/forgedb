Ask for an exclusive commit turn.

The coordinator checks `write_set_keys` against the keys committed after `snapshot_lsn` and replies [`ServerMsg::Grant`] or [`ServerMsg::Nack`]. If a turn is already outstanding it waits for that turn to be released, up to `min(turn_timeout, client_deadline_ms - 500ms)`, and replies [`ServerMsg::Busy`] if the wait expires. No column state is touched; the data-plane write remains the client's job.
