Encode to the self-describing binary wire frame — **identical** to the
durable on-disk framing, so the replication transport and the log share
one codec. The transport sends exactly one frame per message; a follower
decodes with [`from_wire`](PersistedEvent::from_wire).

Field-blind: `bytes` are copied verbatim, never interpreted.
