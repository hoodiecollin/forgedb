#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    watermark: usize,
}

impl Snapshot {
    pub fn new(row_count: usize) -> Self {
        Self {
            watermark: row_count,
        }
    }

    pub fn watermark(&self) -> usize {
        self.watermark
    }

    pub fn visible(&self, index: usize) -> bool {
        index < self.watermark
    }
}
