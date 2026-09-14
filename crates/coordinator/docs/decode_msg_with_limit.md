Decode one length-framed JSON message, rejecting frames larger than
`max_frame` (#145) — the coordinator threads its configured
[`server::CoordConfig::max_frame`] here to bound per-turn write-set memory.
