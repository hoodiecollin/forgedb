Headroom subtracted from a client's declared deadline when computing how long a `RequestTurn` may wait for a free turn: 500 ms.

The wait is `min(turn_timeout, client_deadline - GRANT_REPLY_MARGIN)`, saturating at zero, so the `Busy` reply is written before the client's read timeout fires. It is a constant rather than a fraction because the reply is a small frame on a same-machine socket whose cost does not scale with the deadline.
