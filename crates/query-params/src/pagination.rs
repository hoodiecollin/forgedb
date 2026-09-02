use serde::Deserialize;

pub const DEFAULT_LIMIT: usize = 50;

pub const MAX_LIMIT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            offset: 0,
        }
    }
}

impl Pagination {
    pub fn new(limit: usize, offset: usize) -> Self {
        let limit = limit.clamp(1, MAX_LIMIT);
        Self { limit, offset }
    }

    pub fn from_params(limit: Option<usize>, offset: Option<usize>) -> Self {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        Self { limit, offset }
    }

    pub fn end(&self) -> usize {
        self.offset.saturating_add(self.limit)
    }

    pub fn has_next(&self, total_count: usize) -> bool {
        self.end() < total_count
    }

    pub fn next_page(&self) -> Self {
        Self {
            limit: self.limit,
            offset: self.offset + self.limit,
        }
    }

    pub fn prev_page(&self) -> Option<Self> {
        if self.offset == 0 {
            None
        } else {
            Some(Self {
                limit: self.limit,
                offset: self.offset.saturating_sub(self.limit),
            })
        }
    }

    pub fn apply<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.offset.min(items.len());
        let end = self.end().min(items.len());
        &items[start..end]
    }
}
