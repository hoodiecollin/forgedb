The client's own I/O timeout in milliseconds; the coordinator clamps its grant wait to fit inside it, less [`server::GRANT_REPLY_MARGIN`], so a `Busy` reply is written before the client stops reading.

`0`, the serde default for a client that omits the field, means "unknown" and is treated as 35 seconds. It is an additive field rather than a handshake message because the protocol has no version field: an unknown field is ignored by both peers, an unknown variant is not.
