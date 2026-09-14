Stable on-disk / on-wire byte encoding.

These byte values are a **durable format contract** for [`durable`] —
they are persisted to the broker log and sent across a process boundary,
so they must never be reordered or reused. Append new variants with new
byte values only.
