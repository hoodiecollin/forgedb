Announce that the data-plane column + WAL write is now durable.

The coordinator appends the opaque payload to `_replication.log` and
releases the outstanding turn.
