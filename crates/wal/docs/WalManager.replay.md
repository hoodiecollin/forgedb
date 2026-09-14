Replay every entry from the WAL in file order, passing each to
`callback`.

The replay is **schema-blind**: entries are returned as `WalEntry`
values carrying opaque `Raw` payloads. No decode step is applied.
Stops at the first corrupt or incomplete entry (torn tail) and returns
the entries recovered up to that point.
