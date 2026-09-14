The stable byte encoding: `Inserted` = 0, `Updated` = 1, `Deleted` = 2,
`Linked` = 3.

These values are persisted in the [`durable`] broker log and sent over the
replication transport, so they must never be reordered or reused; a new
variant takes a new byte. [`Self::from_byte`] is the inverse.
