Ask the coordinator for an exclusive commit turn.

Sends `RequestTurn` with the opaque `write_set_keys`, the `snapshot_lsn` the transaction read at, and this client's I/O timeout as its declared deadline, then blocks for the reply. Returns `Ok((turn_id, reserved_lsn))` on a `Grant`. A `Nack` becomes [`ClientError::Conflict`], a `Busy` becomes [`ClientError::Busy`], a server `Error` or any unexpected reply becomes [`ClientError::Protocol`], and an I/O failure becomes [`ClientError::Io`] and poisons the connection.
