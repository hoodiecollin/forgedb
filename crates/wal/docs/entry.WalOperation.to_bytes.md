Serialize the operation payload to bytes (excluding the type byte and
model-name framing, which are written by [`WalEntry::to_bytes`]).
