Current length of the WAL file in bytes, read from file metadata.

Appends are not buffered in user space, so the value already includes records
appended but not yet fsynced. It is the mark [`Self::truncate_to`] rolls back
to.
