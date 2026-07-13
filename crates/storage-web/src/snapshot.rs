//! Read snapshot — a bare row-count watermark, identical to the native
//! `Snapshot`. Because the engine is append-only, a row's position is stable for
//! its lifetime, so a single integer defines a consistent view. Schema-agnostic:
//! it knows only row positions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    watermark: usize,
}

impl Snapshot {
    /// Capture a snapshot at the given committed row count.
    pub fn new(row_count: usize) -> Self {
        Self {
            watermark: row_count,
        }
    }

    /// The captured row count. Rows at index `0..watermark` are visible.
    pub fn watermark(&self) -> usize {
        self.watermark
    }

    /// Whether the row at `index` was committed as of this snapshot.
    pub fn visible(&self, index: usize) -> bool {
        index < self.watermark
    }
}
