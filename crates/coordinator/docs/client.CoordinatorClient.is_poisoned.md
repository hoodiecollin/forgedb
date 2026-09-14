Whether a failed request has left this connection unusable.

A request that fails with an I/O error (a timeout, a broken pipe, an undecodable frame) may leave a reply stranded on the socket, so the client refuses every further request with an `Io` error carrying [`POISONED_MSG`] until [`Self::reconnect`] succeeds. A `Conflict`, `Busy` or `Protocol` result does not poison the connection.
