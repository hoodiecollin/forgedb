One WAL record: a model-name tag plus a [`WalOperation`].

`model_name` is stored verbatim in the record header and never interpreted by
this crate; the caller uses it to route replayed records. [`Self::to_bytes`] and
[`Self::from_bytes`] are the on-disk framing, CRC32 included.
