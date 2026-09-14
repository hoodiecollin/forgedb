Releases a snapshot registered by [`Self::register_snapshot`].

Decrements the reference count for `s` and removes the entry when it reaches zero, which lets
[`Self::gc`] prune further. Releasing an LSN that is not live is a no-op.
