The slot's value **borrowed from the buffered span** — no allocation, no
copy (#224).

#222 made the span an `mmap` alias of the data region, so a scan already
holds every live string's bytes; this is the read that stops copying them
back out. The borrow is tied to `&self` and the mapping is owned by `self`,
so the compiler guarantees the `&str` cannot outlive the pages it points at
— a *tighter* constraint than the type already operates under, not a new
aliasing exposure.

UTF-8 is validated on every read (cheap next to the allocation it replaces)
so an on-disk corruption surfaces as [`io::ErrorKind::InvalidData`] rather
than as an unchecked `&str`.
