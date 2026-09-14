Write-ahead log substrate: CRC-framed opaque byte records appended to a single file.

`forgedb-wal` is schema-agnostic. A [`WalEntry`] carries a model-name tag and a
[`WalOperation::Raw`] payload; the crate frames, checksums, appends and replays
those bytes and never interprets them. The caller owns the payload encoding.

## Types

- [`WalManager`] — one WAL file: append, fsync, replay, truncate, rotate.
- [`WalWriter`] / [`WalReader`] — the append and decode halves the manager wraps.
- [`FsyncPolicy`] — whether an append is followed by an fsync.
- [`CorruptionInfo`] — where and why decoding failed, from
  [`WalReader::read_with_validation`].

## Record layout

Every record is written as:

```text
[4 bytes LE u32]  total length of everything after this field
[1 byte]          operation type byte (0x20 = Raw)
[2 bytes LE u16]  model name length
[N bytes]         model name (UTF-8, stored verbatim)
[M bytes]         operation data
[4 bytes LE u32]  CRC32 over the type byte through the operation data
```

The `Raw` operation's data is `[4 bytes LE u32 payload length][payload]`. The
CRC does not cover the leading length field.

## Crash safety

[`WalReader::read_all`] (and so [`WalManager::replay`]) decodes records from the
start of the file and stops at the first one that is incomplete or fails its
checksum, returning the valid prefix. An append interrupted mid-record costs
only that record. Records are never rewritten in place; bytes leave the file
only through [`WalManager::truncate`], [`WalManager::truncate_to`] and
[`WalManager::rotate`].
