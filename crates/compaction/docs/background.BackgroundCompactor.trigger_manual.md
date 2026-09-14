Trigger a one-shot manual compaction in a new thread (non-blocking).

# C6 fix

The previous implementation checked status then spawned in separate steps,
creating a TOCTOU race where two concurrent callers could both observe
`!Running` and both spawn compaction threads.  Now the check and the
`Running` transition happen inside a single mutex critical section.
