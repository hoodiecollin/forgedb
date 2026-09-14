Connect to the coordinator at `socket_path` with an explicit I/O timeout.

`io_timeout` is set as both the read and the write timeout on the socket, and its millisecond value is declared to the coordinator as `client_deadline_ms` on every `RequestTurn`. The coordinator clamps how long it waits for a free turn to `min(turn_timeout, io_timeout - 500ms)` (the margin is [`crate::server::GRANT_REPLY_MARGIN`]), so a `Busy` reply arrives before this client stops reading; lowering the value makes both sides give up sooner together.

Fails with the underlying connect error when no coordinator is listening at the path.
