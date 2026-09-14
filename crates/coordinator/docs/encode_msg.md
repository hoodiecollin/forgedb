Serialize `msg` as JSON and prefix it with its length.

The frame layout is:

```text
[u32 payload length, big-endian][JSON payload]
```

Fails with `InvalidData` if serialization fails. The length is not checked against any maximum here; that check happens at decode.
