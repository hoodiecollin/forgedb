use tokio::sync::broadcast;

pub mod durable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Inserted,
    Updated,
    Deleted,
    Linked,
}

impl ChangeKind {
    pub fn to_byte(self) -> u8 {
        match self {
            ChangeKind::Inserted => 0,
            ChangeKind::Updated => 1,
            ChangeKind::Deleted => 2,
            ChangeKind::Linked => 3,
        }
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(ChangeKind::Inserted),
            1 => Some(ChangeKind::Updated),
            2 => Some(ChangeKind::Deleted),
            3 => Some(ChangeKind::Linked),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeEvent {
    pub model: &'static str,
    pub row_index: usize,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub struct ChangeFeed {
    sender: broadcast::Sender<ChangeEvent>,
}

impl ChangeFeed {
    pub fn new(capacity: usize) -> Self {
        let (sender, _rx) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn emit(&self, model: &'static str, row_index: usize, kind: ChangeKind) -> usize {
        self.sender
            .send(ChangeEvent {
                model,
                row_index,
                kind,
            })
            .unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.sender.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for ChangeFeed {
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
        assert_eq!(feed.emit("User", 0, ChangeKind::Inserted), 0);
        assert_eq!(feed.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn clone_shares_the_same_channel() {
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

    #[tokio::test]
    async fn mutation_kinds_carry_through_the_feed() {
        let feed = ChangeFeed::new(16);
        let mut rx = feed.subscribe();
        feed.emit("Post", 7, ChangeKind::Updated);
        feed.emit("Post", 3, ChangeKind::Deleted);
        let updated = rx.recv().await.unwrap();
        let deleted = rx.recv().await.unwrap();
        assert_eq!(updated, ChangeEvent { model: "Post", row_index: 7, kind: ChangeKind::Updated });
        assert_eq!(deleted, ChangeEvent { model: "Post", row_index: 3, kind: ChangeKind::Deleted });
    }
}
