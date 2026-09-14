Default I/O timeout for individual read/write operations.

This value is **declared to the coordinator** on every `RequestTurn`
(`client_deadline_ms`), which clamps its grant wait to fit inside it (#274).
Before #274 the coordinator could not see it, so raising `--turn-timeout` past
this value silently desynchronized the connection.
