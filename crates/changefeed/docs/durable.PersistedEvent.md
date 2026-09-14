A single durable, offset-addressed change record, and the frame it is stored
and transmitted as.

Field-blind by construction: `model` is an opaque routing tag and `bytes` are
the committed row bytes carried verbatim. [`PersistedEvent::to_wire`] and
[`PersistedEvent::from_wire`] use exactly the framing the broker log uses, so
one codec serves both the log and the replication transport. All integers are
little-endian:

```text
u32  total_len      payload length + 4
     payload
       u64  offset
       u64  row_index
       u8   kind       ChangeKind::to_byte
       u16  model_len
       ...  model      model_len bytes of UTF-8
       u32  bytes_len
       ...  bytes      bytes_len opaque bytes
u32  crc32          CRC-32 of the payload
```
