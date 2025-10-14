//! Sort parameter handling

use serde::Deserialize;

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

impl SortOrder {
    /// Parse sort order from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Some(SortOrder::Asc),
            "desc" | "descending" => Some(SortOrder::Desc),
            _ => None,
        }
    }
}

/// Sort represents sorting parameters
#[derive(Debug, Clone, PartialEq)]
pub struct Sort {
    pub field: String,
    pub order: SortOrder,
}

impl Sort {
    /// Create a new sort
    pub fn new(field: impl Into<String>, order: SortOrder) -> Self {
        Self {
            field: field.into(),
            order,
        }
    }

    /// Parse from query parameters
    pub fn from_params(sort_field: Option<String>, order_str: Option<String>) -> Option<Self> {
        sort_field.map(|field| {
            let order = order_str
                .and_then(|s| SortOrder::from_str(&s))
                .unwrap_or_default();
            Sort { field, order }
        })
    }

    /// Check if sort is ascending
    pub fn is_ascending(&self) -> bool {
        matches!(self.order, SortOrder::Asc)
    }

    /// Check if sort is descending
    pub fn is_descending(&self) -> bool {
        matches!(self.order, SortOrder::Desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_order_from_str() {
        assert_eq!(SortOrder::from_str("asc"), Some(SortOrder::Asc));
        assert_eq!(SortOrder::from_str("ASC"), Some(SortOrder::Asc));
        assert_eq!(SortOrder::from_str("ascending"), Some(SortOrder::Asc));
        assert_eq!(SortOrder::from_str("desc"), Some(SortOrder::Desc));
        assert_eq!(SortOrder::from_str("DESC"), Some(SortOrder::Desc));
        assert_eq!(SortOrder::from_str("descending"), Some(SortOrder::Desc));
        assert_eq!(SortOrder::from_str("invalid"), None);
    }

    #[test]
    fn test_sort_creation() {
        let sort = Sort::new("name", SortOrder::Asc);
        assert_eq!(sort.field, "name");
        assert_eq!(sort.order, SortOrder::Asc);
    }

    #[test]
    fn test_from_params() {
        let sort = Sort::from_params(Some("name".to_string()), Some("desc".to_string()));
        assert!(sort.is_some());
        let sort = sort.unwrap();
        assert_eq!(sort.field, "name");
        assert_eq!(sort.order, SortOrder::Desc);
    }

    #[test]
    fn test_from_params_default_order() {
        let sort = Sort::from_params(Some("name".to_string()), None);
        assert!(sort.is_some());
        let sort = sort.unwrap();
        assert_eq!(sort.order, SortOrder::Asc);
    }

    #[test]
    fn test_from_params_no_field() {
        let sort = Sort::from_params(None, Some("desc".to_string()));
        assert!(sort.is_none());
    }

    #[test]
    fn test_is_ascending_descending() {
        let asc = Sort::new("name", SortOrder::Asc);
        assert!(asc.is_ascending());
        assert!(!asc.is_descending());

        let desc = Sort::new("name", SortOrder::Desc);
        assert!(!desc.is_ascending());
        assert!(desc.is_descending());
    }
}
