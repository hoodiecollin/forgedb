Connect with an explicit I/O deadline.

The value is declared to the coordinator on every `RequestTurn`, which
clamps its grant wait to `min(turn_timeout, io_timeout - margin)` so a
`Busy` reply always beats this deadline (#274).  Lowering it therefore
makes the client give up sooner *and* makes the coordinator answer sooner —
the two stay coupled, which is the whole point.
