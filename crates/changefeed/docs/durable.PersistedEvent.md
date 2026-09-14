A single durable, offset-addressed change record.

Field-blind by construction: `model` is an opaque routing tag and `bytes` are
opaque committed row bytes carried verbatim. This struct is the on-wire /
on-disk replication frame — it must never gain a field-typed member.
