The operation half of a [`WalEntry`].

A single variant, [`Self::Raw`], exists: the WAL frames and checksums the bytes
the caller hands it and never interprets them. [`Self::type_byte`],
[`Self::to_bytes`] and [`Self::from_bytes`] are the on-disk encoding of this
half of the record.
