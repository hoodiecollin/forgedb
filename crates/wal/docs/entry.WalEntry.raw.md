Create a raw opaque-bytes entry.

`model_name` is an opaque routing/debug tag stored verbatim in the
entry header. The WAL never interprets it. The generated caller uses it
to identify which model a replayed entry belongs to.

`payload` is stored verbatim and returned byte-identical on replay. The
WAL neither inspects nor encodes the payload — the caller owns the
encoding entirely.
