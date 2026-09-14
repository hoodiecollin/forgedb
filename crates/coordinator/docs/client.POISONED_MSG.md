The message carried by the [`ClientError::Io`] a poisoned [`CoordinatorClient`] returns.

Public so a caller can tell "this connection is unusable, call [`CoordinatorClient::reconnect`]" apart from an ordinary I/O failure without a dedicated `ClientError` variant.
