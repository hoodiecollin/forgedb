use serde::Deserialize;

#[doc = include_str!("../docs/pagination.DEFAULT_LIMIT.md")]
pub const DEFAULT_LIMIT: usize = 50;

#[doc = include_str!("../docs/pagination.MAX_LIMIT.md")]
pub const MAX_LIMIT: usize = 1000;

#[doc = include_str!("../docs/pagination.Pagination.md")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Pagination {
    #[doc = include_str!("../docs/pagination.Pagination.limit.md")]
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[doc = include_str!("../docs/pagination.Pagination.offset.md")]
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
    #[doc = include_str!("../docs/pagination.Pagination.new.md")]
    pub fn new(limit: usize, offset: usize) -> Self {
        let limit = limit.clamp(1, MAX_LIMIT);
        Self { limit, offset }
    }

    #[doc = include_str!("../docs/pagination.Pagination.from_params.md")]
    pub fn from_params(limit: Option<usize>, offset: Option<usize>) -> Self {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        Self { limit, offset }
    }

    #[doc = include_str!("../docs/pagination.Pagination.end.md")]
    pub fn end(&self) -> usize {
        self.offset.saturating_add(self.limit)
    }

    #[doc = include_str!("../docs/pagination.Pagination.has_next.md")]
    pub fn has_next(&self, total_count: usize) -> bool {
        self.end() < total_count
    }

    #[doc = include_str!("../docs/pagination.Pagination.next_page.md")]
    pub fn next_page(&self) -> Self {
        Self {
            limit: self.limit,
            offset: self.offset + self.limit,
        }
    }

    #[doc = include_str!("../docs/pagination.Pagination.prev_page.md")]
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

    #[doc = include_str!("../docs/pagination.Pagination.apply.md")]
    pub fn apply<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.offset.min(items.len());
        let end = self.end().min(items.len());
        &items[start..end]
    }
}
