MVCC Tier 3 commit coordinator for ForgeDB: a schema-agnostic control plane that serializes commit turns for several writer processes sharing one data directory.

One coordinator process (`forgedb coordinate <root>`) serves one data directory. It holds the directory's single-writer lock on behalf of every coordinated client, runs the conflict check and LSN sequence through [`forgedb_txn::CommitSequencer`], appends each committed payload to `<root>/_coordinator_replication.log` through [`forgedb_changefeed::durable::DurableBroker`], and hands out exclusive commit turns over a Unix domain socket. The server side is [`server`], the client side generated writers link is [`client`], and the wire protocol is defined at the crate root.

## Turn protocol

Every frame is a length-prefixed JSON message ([`encode_msg`] / [`decode_msg`]); [`ClientMsg`] and [`ServerMsg`] are internally tagged by a `type` field holding the variant name. A commit takes three steps:

1. The client sends [`ClientMsg::RequestTurn`] with its opaque write-set keys and read-snapshot LSN. The coordinator checks the keys against those committed after that LSN and replies [`ServerMsg::Grant`] (an exclusive turn plus the reserved LSN), [`ServerMsg::Nack`] (a conflict; retry from a fresh snapshot) or [`ServerMsg::Busy`] (another turn stayed outstanding for the whole wait window).
2. Holding the grant, the client performs its own data-plane write (columns and WAL) and makes it durable.
3. The client sends [`ClientMsg::Committed`] with the opaque row bytes. The coordinator releases the turn, appends the rows to the replication log, and replies [`ServerMsg::Ack`].

At most one turn is outstanding at a time; a turn not committed within the configured timeout is reclaimed.

## What the coordinator never does

It never opens a column file, never decodes `opaque_row_bytes`, and treats model names as opaque tags. It has no dependency on any `forgedb-storage` crate, and the crate is `#![forbid(unsafe_code)]`.
