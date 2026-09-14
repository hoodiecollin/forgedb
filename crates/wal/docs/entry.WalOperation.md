WAL operation types.

Only the `Raw` variant remains. The WAL stores opaque bytes and never
interprets their content. The (generated) caller owns the encoding; the WAL
provides framing, CRC integrity, and torn-tail crash safety.
