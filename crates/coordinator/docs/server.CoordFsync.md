When the coordinator fsyncs `<root>/_coordinator_replication.log`.

The broker is opened with `FsyncPolicy::Never`; the coordinator itself issues at most one flush per commit, after all of that commit's events are appended, as this policy dictates. `Default` is [`CoordFsync::Always`]. Selected with `forgedb coordinate --fsync always|never|periodic` (the periodic interval comes from `--fsync-interval`, default 64).

The log is a resumable secondary artifact: a client sends `Committed` only after its own columns and WAL are durable, so relaxing this policy never loses committed client data. A coordinator crash before a flush loses only the unflushed tail of the replication log.
