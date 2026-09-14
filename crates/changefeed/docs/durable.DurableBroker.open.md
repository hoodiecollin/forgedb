Open or create a broker whose durable log lives at `path`.

An existing log is scanned torn-tail-safe to recover the current
watermark and earliest retained offset, so offsets stay monotonic across
restarts. `capacity` bounds each live subscriber's in-memory ring (lag
past it drops the oldest live events — a lagging follower falls back to
durable replay, which is the whole point).
