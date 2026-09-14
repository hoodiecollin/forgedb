Dial the coordinator again, replace the socket, and clear the poison flag.

The old socket is sent a best-effort `Disconnect` before it is dropped, which discards any reply stranded on it; [`Self::last_known_lsn`] is kept. Takes `&self` so it can be called through the `Arc` a generated writer holds; whether and when to reconnect is the caller's policy.

If dialing fails the error is returned, the old socket stays in place, and the poison flag stays set.
