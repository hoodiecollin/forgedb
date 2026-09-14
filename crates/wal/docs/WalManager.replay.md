Decode every record from the start of the file, pass each to `callback` in
file order, and return them all.

Reading is [`WalReader::read_all`]: it stops at the first incomplete or
checksum-failing record and keeps the valid prefix, so a torn tail is dropped
rather than reported. Entries come back as [`WalEntry`] values with their `Raw`
payload bytes verbatim; nothing beyond the framing is decoded. The callbacks run
only after the whole file has been read, and the first `Err` from `callback`
skips the remaining callbacks and is returned.
