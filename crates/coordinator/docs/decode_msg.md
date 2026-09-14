Read one length-prefixed JSON frame from `reader`, rejecting a payload larger than [`DEFAULT_MAX_FRAME`].

Equivalent to [`decode_msg_with_limit`] with that default. The client uses it for coordinator replies.
