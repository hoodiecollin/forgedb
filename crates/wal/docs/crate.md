Write-Ahead Log (WAL) Implementation

`forgedb-wal` is a **schema-agnostic substrate** crate. It stores and
returns opaque byte records and knows nothing about any application schema.

## Wire format

Each entry on disk:
```text
[4 bytes: total entry length (excluding this field)]
[1 byte: operation type]
[2 bytes: model name length]
[N bytes: model name (UTF-8, opaque routing tag)]
[M bytes: serialized operation data]
[4 bytes: CRC32 checksum]
```

Only one operation type exists:
- `Raw` (`0x20`): `[payload_len (4 bytes LE)][payload_bytes...]`

The WAL breaks at the first corrupt or incomplete entry and returns the
valid prefix (torn-tail crash safety).
