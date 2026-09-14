Encode the operation data: for [`Self::Raw`], a little-endian `u32` payload
length followed by the payload. Excludes the type byte and model-name framing,
which [`WalEntry::to_bytes`] writes.
