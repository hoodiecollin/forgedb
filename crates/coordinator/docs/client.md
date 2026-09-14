Schema-agnostic client for the Tier 3 coordinator Unix socket.

The generated `CoordinatedDatabase` wraps this client; the client itself
knows nothing about models, fields, or schema — it only speaks the
coordinator wire protocol.
