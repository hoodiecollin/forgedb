The commit sequencer: a monotonic LSN source plus a first-committer-wins conflict map.

The state is in-memory and ephemeral. It rebuilds empty when the owning process restarts,
which is sound because in-flight transactions do not survive a restart either: the conflict map
only needs to cover transactions that are still live.

Callers wrap it in a mutex; every method takes `&mut self`.
