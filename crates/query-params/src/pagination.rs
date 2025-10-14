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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pagination() {
        let p = Pagination::default();
        assert_eq!(p.limit, DEFAULT_LIMIT);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn test_new_pagination() {
        let p = Pagination::new(100, 50);
        assert_eq!(p.limit, 100);
        assert_eq!(p.offset, 50);
    }

    #[test]
    fn test_new_pagination_clamps_limit() {
        // Too high
        let p = Pagination::new(2000, 0);
        assert_eq!(p.limit, MAX_LIMIT);

        // Too low
        let p = Pagination::new(0, 0);
        assert_eq!(p.limit, 1);
    }

    #[test]
    fn test_from_params() {
        let p = Pagination::from_params(Some(100), Some(50));
        assert_eq!(p.limit, 100);
        assert_eq!(p.offset, 50);
    }

    #[test]
    fn test_from_params_defaults() {
        let p = Pagination::from_params(None, None);
        assert_eq!(p.limit, DEFAULT_LIMIT);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn test_end() {
        let p = Pagination::new(50, 100);
        assert_eq!(p.end(), 150);
    }

    #[test]
    fn test_has_next() {
        let p = Pagination::new(50, 0);
        assert!(p.has_next(100));
        assert!(!p.has_next(30));
        assert!(!p.has_next(50));
    }

    #[test]
    fn test_next_page() {
        let p = Pagination::new(50, 0);
        let next = p.next_page();
        assert_eq!(next.offset, 50);
        assert_eq!(next.limit, 50);
    }

    #[test]
    fn test_prev_page() {
        let p = Pagination::new(50, 100);
        let prev = p.prev_page();
        assert!(prev.is_some());
        let prev = prev.unwrap();
        assert_eq!(prev.offset, 50);
        assert_eq!(prev.limit, 50);
    }

    #[test]
    fn test_prev_page_at_start() {
        let p = Pagination::new(50, 0);
        assert!(p.prev_page().is_none());
    }

    #[test]
    fn test_apply() {
        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let p = Pagination::new(3, 2);
        let result = p.apply(&items);
        assert_eq!(result, &[3, 4, 5]);
    }

    #[test]
    fn test_apply_beyond_end() {
        let items = vec![1, 2, 3];
        let p = Pagination::new(10, 1);
        let result = p.apply(&items);
        assert_eq!(result, &[2, 3]);
    }

    #[test]
    fn test_apply_offset_beyond_end() {
        let items = vec![1, 2, 3];
        let p = Pagination::new(10, 10);
        let result = p.apply(&items);
        assert_eq!(result, &[]);
    }
}
