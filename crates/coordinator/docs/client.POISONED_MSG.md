The message carried by the [`ClientError::Io`] a poisoned client returns.

Stable and public so a caller can distinguish "this connection is unusable,
call `reconnect`" from an ordinary I/O failure without a new `ClientError`
variant — which would be a breaking change to a non-`non_exhaustive` public
enum, and would invalidate the `forgedb-coordinator = "0.2"` scaffold pin in
five generated `Cargo.toml`s for no behavioral gain.
