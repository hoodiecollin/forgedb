The default [`CoordConfig::turn_timeout`]: 30 seconds.

A client that crashes or hangs while holding a turn loses it once this much time has passed. The reclaim happens lazily, on the next `RequestTurn`, and a late `Committed` for the reclaimed turn is answered with `Error`.
