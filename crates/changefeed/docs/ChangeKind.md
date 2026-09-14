What kind of change a [`ChangeEvent`] or [`durable::PersistedEvent`] describes.

Every kind is an append in the storage engine, so the accompanying `row_index`
always names a committed row; which row a generated emitter passes is its
concern, and this crate never decodes it. [`ChangeKind::to_byte`] and
[`ChangeKind::from_byte`] fix the byte encoding the durable log and the
replication transport use.
