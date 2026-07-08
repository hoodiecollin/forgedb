//! `forgedb-changefeed` — a schema-agnostic, in-process change-feed broadcast
//! primitive.
//!
//! This is **Class-1 substrate**: a peer of `forgedb-storage` / `forgedb-wal`
//! that knows nothing about any `.forge` schema. It carries only a *positional*
//! signal — "model `M` gained a row at index `N`" — and never decodes a record,
//! reads a field, or filters by field value. Those responsibilities live in
//! **generated code**: the generated `insert()` / `link_*` methods (which know
//! the model's identity and hold the typed record) emit into this feed, and a
//! generated per-model WebSocket handler turns a `(model, row_index)` signal
//! back into a typed payload and applies any generated per-model filter.
//!
//! The append-only storage engine *is* a change log — every write is positionally
//! an event. This primitive simply fans that fact out to subscribers, best-effort
//! and in-process. Durable replay, offsets, and cross-process fan-out are
//! explicitly out of scope (Direction C in the realtime-subscriptions proposal).
//!
//! ## The red line
//!
//! `ChangeEvent` holds a `&'static str` model name and a `usize` row index — no
//! field data, ever. If this crate ever needed to inspect a record's contents to
//! route or filter an event, it would have become the forbidden generic engine,
//! event-shaped. It does not: routing by model name and materialization of the
//! typed payload both happen in generated code.

use tokio::sync::broadcast;

/// What kind of change a [`ChangeEvent`] describes.
///
/// **Insert-only today**, mirroring the append-only storage engine: a model row
/// append is `Inserted`, an M2M junction append is `Linked`. `Updated` / `Deleted`
/// are intentionally absent — they are gated on the generated mutation surface and
/// retraction primitive (see the `mvcc-concurrency` and `realtime-subscriptions`
/// proposals). Adding them here before that lands would be faking events the
/// storage engine cannot yet produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A record was appended to a model's storage.
    Inserted,
    /// A pair was appended to an M2M junction table.
    Linked,
}

/// A single positional change signal: model `model` gained a row at `row_index`.
///
/// Field-blind by construction — it carries the model's name (a generated
/// `&'static str`) and the append position, nothing else. A subscriber that wants
/// the actual record materializes it in generated code from `row_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeEvent {
    /// The model (or junction) name, e.g. `"User"` or `"post_tag_link"`. Always a
    /// generated `&'static str` — never derived from record contents.
    pub model: &'static str,
    /// The append position of the new row in that collection's storage.
    pub row_index: usize,
    /// Whether this was a model insert or an M2M link.
    pub kind: ChangeKind,
}

/// An in-process, best-effort, bounded broadcast feed of [`ChangeEvent`]s.
///
/// Backed by [`tokio::sync::broadcast`]: any number of independent subscribers,
/// each with its own bounded ring buffer. A subscriber that lags past the buffer
/// drops the oldest events rather than blocking a writer — the feed is best-effort
/// and never applies backpressure to the insert path. Cloning a `ChangeFeed`
/// shares the same underlying channel, so a clone handed to a per-model storage
/// publishes to the same subscribers as the original.
#[derive(Debug, Clone)]
pub struct ChangeFeed {
    sender: broadcast::Sender<ChangeEvent>,
}

impl ChangeFeed {
    /// Create a feed whose per-subscriber buffer holds up to `capacity` events.
    ///
    /// `capacity` bounds the lag a slow subscriber may accumulate before it starts
    /// dropping the oldest events; it does not bound the number of subscribers.
    pub fn new(capacity: usize) -> Self {
        let (sender, _rx) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish a change signal to all current subscribers.
    ///
    /// Best-effort and non-blocking: returns the number of subscribers the event
    /// reached (`0` when there are none — not an error). A writer never waits on a
    /// subscriber.
    pub fn emit(&self, model: &'static str, row_index: usize, kind: ChangeKind) -> usize {
        self.sender
            .send(ChangeEvent {
                model,
                row_index,
                kind,
            })
            .unwrap_or(0)
    }

    /// Subscribe to the feed. The returned receiver observes every event published
    /// *after* this call.
    pub fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.sender.subscribe()
    }

    /// The number of live subscribers (receivers) currently attached.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for ChangeFeed {
    /// A feed with a 1024-event per-subscriber buffer — the generated-server default.
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscriber_receives_emitted_events_in_order() {
        let feed = ChangeFeed::new(16);
        let mut rx = feed.subscribe();

        feed.emit("User", 0, ChangeKind::Inserted);
        feed.emit("User", 1, ChangeKind::Inserted);
        feed.emit("post_tag_link", 0, ChangeKind::Linked);

        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        let c = rx.recv().await.unwrap();
        assert_eq!(a, ChangeEvent { model: "User", row_index: 0, kind: ChangeKind::Inserted });
        assert_eq!(b, ChangeEvent { model: "User", row_index: 1, kind: ChangeKind::Inserted });
        assert_eq!(c.model, "post_tag_link");
        assert_eq!(c.kind, ChangeKind::Linked);
    }

    #[tokio::test]
    async fn emit_with_no_subscribers_is_not_an_error() {
        let feed = ChangeFeed::new(16);
        // No receivers yet: emit reaches 0 subscribers but does not panic/err.
        assert_eq!(feed.emit("User", 0, ChangeKind::Inserted), 0);
        assert_eq!(feed.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn clone_shares_the_same_channel() {
        // A clone (as handed to a per-model storage) publishes to subscribers of
        // the original — they are the same underlying feed.
        let feed = ChangeFeed::new(16);
        let mut rx = feed.subscribe();
        let clone = feed.clone();
        clone.emit("Post", 7, ChangeKind::Inserted);
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.model, "Post");
        assert_eq!(ev.row_index, 7);
    }

    #[tokio::test]
    async fn independent_subscribers_each_get_their_own_stream() {
        let feed = ChangeFeed::new(16);
        let mut rx1 = feed.subscribe();
        let mut rx2 = feed.subscribe();
        feed.emit("User", 42, ChangeKind::Inserted);
        assert_eq!(rx1.recv().await.unwrap().row_index, 42);
        assert_eq!(rx2.recv().await.unwrap().row_index, 42);
    }
}
