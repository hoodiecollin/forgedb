Build an entry carrying `payload` as a [`WalOperation::Raw`] operation tagged
`model_name`.

Both are stored verbatim: the name as the record's routing tag, the payload as
bytes this crate neither inspects nor re-encodes. The caller owns the payload
encoding.
