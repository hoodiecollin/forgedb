In-memory commit sequencer implementing snapshot isolation with
first-committer-wins (SI-FCW, #83 Tier 2).

The sequencer is intentionally **in-memory and ephemeral**: it rebuilds
empty on process restart.  In-flight transactions do not survive a restart
(they are rolled back by the Tier-1 WAL journal), so the conflict map does
not need to be persisted.

`CommitSequencer` is behind `Arc<Mutex<..>>` in the generated `Database`
so that a future Tier 3 path may hold the lock only for the serialized
commit point rather than across the entire prepare phase.
