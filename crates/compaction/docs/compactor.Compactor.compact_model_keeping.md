Compact a model keeping EXACTLY the physical rows named in `keep`
(any order; out-of-range indices ignored), dropping every other row and
renumbering the survivors densely.

Schema-agnostic: the CALLER decides which rows are live and hands over an
opaque set of row indices — this substrate only rewrites bytes, it reads no
schema.  This is the reclaim primitive the generated in-process auto-
compaction (#92) drives: for the #66 superseding-version mutation surface,
the generated code passes the newest non-tombstoned row per live id, so
orphaned old versions AND deleted rows are simply omitted from `keep`.  It
exists because the tombstone-only `compact_model` cannot express "this
non-tombstoned row is a superseded dead version" and would resurrect #66
deletes.
