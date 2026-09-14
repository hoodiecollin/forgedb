Encode the entry as one framed record, ready to append.

Layout, all integers little-endian:

```text
[4: total length][1: type byte][2: model name length][N: model name][M: operation data][4: CRC32]
```

The leading length counts every byte after itself, checksum included; the
CRC32 covers the type byte through the operation data.
