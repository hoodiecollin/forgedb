Read-only decoder over a WAL file.

Opened on an existing file by [`Self::new`]. [`Self::read_all`] and
[`Self::read_with_validation`] decode from the start of the file;
[`Self::read_one`], [`Self::position`] and [`Self::seek`] step through it one
record at a time. It never writes.
