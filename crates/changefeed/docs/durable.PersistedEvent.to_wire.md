Encode this event as one self-describing binary frame, byte-identical to its
representation in the broker log (layout: [`PersistedEvent`]).

The generated replication transport sends exactly one frame per message; a
follower decodes it with [`PersistedEvent::from_wire`]. `bytes` are copied
verbatim.
