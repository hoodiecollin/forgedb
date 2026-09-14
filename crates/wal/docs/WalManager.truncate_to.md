Truncate the WAL to a specific byte `offset`, dropping every byte after it
(MVCC Tier 1 transaction rollback).

Unlike [`truncate`] (which clears the whole log), this rolls the file back
to a previously-recorded [`size`] mark, so a committed WAL prefix survives
and only the aborted transaction's staged records are discarded.
A no-op if `offset` is already `>=` the current length.

[`truncate`]: WalManager::truncate
[`size`]: WalManager::size
