What kind of change a [`ChangeEvent`] describes.

Every kind still maps to an **append** — the storage engine is append-only and
this primitive stays a positional signal. A model insert is `Inserted`; an M2M
junction append is `Linked`. The mutation surface (#66, superseding-version
append) adds `Updated` / `Deleted`: both are *also* appends (a new version of a
row, tombstoned for a delete), so `row_index` still points at a committed row —
for `Updated` the new live version, for `Deleted` the pre-delete version that a
subscriber can still materialize (the tombstoned version reads as absent).
Which row a generated emitter passes is its concern; this crate never decodes it.
