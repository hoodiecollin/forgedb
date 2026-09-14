When the coordinator fsyncs its durable replication log at commit (#156,
Option C). Configurable per deployment (`forgedb coordinate --fsync`);
default [`CoordFsync::Always`], which preserves the pre-#156 durability of
the replication log.

The coordinator's `_coordinator_replication.log` is a **resumable, secondary**
artifact: a coordinated client already fsync'd its own columns + WAL before
reporting `Committed`, and followers re-request from their watermark on a
coordinator crash — so trading this log's fsync for throughput never loses
committed client data, only rewinds replication. That is why `Never`/`Periodic`
are safe to offer (guardrail G7 still applies: they are explicit opt-ins).
