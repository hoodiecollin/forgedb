One connection to a running coordinator.

The socket sits behind a `Mutex`, so the client can be shared across threads; because the protocol is strictly one request, one reply, concurrent callers serialize on that lock. All I/O is blocking, with the read and write timeout chosen at connect time. When a request fails with an I/O error the connection is poisoned (see [`Self::is_poisoned`]) and every later request fails with [`POISONED_MSG`] until [`Self::reconnect`] succeeds. Dropping the client sends a best-effort [`crate::ClientMsg::Disconnect`].
