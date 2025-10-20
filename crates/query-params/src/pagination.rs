//! Pagination parameter handling

use serde::Deserialize;

/// Default pagination limit
pub const DEFAULT_LIMIT: usize = 50;

/// Maximum pagination limit
pub const MAX_LIMIT: usize = 1000;

/// Pagination represents limit and offset parameters
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
    /// Create a new pagination
    pub fn new(limit: usize, offset: usize) -> Self {
        let limit = limit.clamp(1, MAX_LIMIT);
        Self { limit, offset }
    }

    /// Parse from query parameters
    pub fn from_params(limit: Option<usize>, offset: Option<usize>) -> Self {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        Self { limit, offset }
    }

    /// Get the end index (offset + limit)
    pub fn end(&self) -> usize {
        self.offset + self.limit
    }

    /// Check if there's a next page
    pub fn has_next(&self, total_count: usize) -> bool {
        self.end() < total_count
    }

    /// Get the next page pagination
    pub fn next_page(&self) -> Self {
        Self {
            limit: self.limit,
            offset: self.offset + self.limit,
        }
    }

    /// Get the previous page pagination
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

    /// Apply pagination to a slice
    pub fn apply<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.offset.min(items.len());
        let end = self.end().min(items.len());
        &items[start..end]
    }
}
