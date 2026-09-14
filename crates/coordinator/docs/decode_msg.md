Decode one length-framed JSON message from a reader, rejecting frames larger
than [`DEFAULT_MAX_FRAME`]. Used by the client to decode (small) server
responses; the coordinator's per-connection decode uses
[`decode_msg_with_limit`] with its configured cap (#145).
