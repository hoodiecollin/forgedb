Decode exactly one wire frame produced by [`to_wire`](PersistedEvent::to_wire).

`Err` on a CRC mismatch, a structurally invalid frame, or a truncated /
incomplete frame (a whole WS message must be one complete frame).
