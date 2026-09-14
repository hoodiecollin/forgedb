Record a committed change, assigning the next global offset.

Durably appends the frame (per the [`FsyncPolicy`]) **before** fanning it
out to live subscribers, then returns the assigned offset. `model` is an
opaque tag and `bytes` are opaque committed row bytes — neither is
interpreted.
