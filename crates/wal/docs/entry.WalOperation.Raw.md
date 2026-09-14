Opaque, schema-agnostic payload. The WAL stores and returns these bytes
verbatim and never interprets them — the (generated) caller owns the
encoding. This is the identity-preserving write path.
