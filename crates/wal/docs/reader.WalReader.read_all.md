Decode every record from the start of the file, stopping at the first that is
incomplete or fails its checksum.

The whole file is read into memory, then decoded record by record with
[`WalEntry::from_bytes`]. The first decode error ends the scan and is not
reported: the entries decoded before it are returned, so a torn tail is
silently dropped. Use [`Self::read_with_validation`] to see where decoding
failed.
