Headroom subtracted from a client's declared deadline, so the coordinator's
`Busy` reply is written **before** the client stops reading (#274).

A fixed constant rather than a fraction of the deadline: the reply is a small
JSON frame over a same-machine Unix socket, and its cost has nothing to do with
how long the operator is willing to wait for a turn.  500ms is orders of
magnitude above the real cost and under 2% of the default [`TURN_TIMEOUT`].
