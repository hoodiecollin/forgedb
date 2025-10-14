//! Query parameter parsing

use crate::{Filter, Pagination, Sort};
use serde::Deserialize;
use std::collections::HashMap;

/// Query parameters parsed from URL query string
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QueryParams {
    #[serde(skip)]
    pub filters: Vec<Filter>,

    #[serde(skip)]
    pub sort: Option<Sort>,

    #[serde(flatten)]
    pub pagination: Pagination,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            filters: vec![],
            sort: None,
            pagination: Pagination::default(),
        }
    }
}

impl QueryParams {
    /// Create new query params with all fields
    pub fn new(filters: Vec<Filter>, sort: Option<Sort>, pagination: Pagination) -> Self {
        Self {
            filters,
            sort,
            pagination,
        }
    }

    /// Parse from a query string
    pub fn from_query_string(query: &str) -> Result<Self, serde_urlencoded::de::Error> {
        let params: HashMap<String, String> = serde_urlencoded::from_str(query)?;
        Ok(Self::from_map(params))
    }

    /// Parse from a HashMap
    pub fn from_map(mut params: HashMap<String, String>) -> Self {
        // Extract special parameters
        let sort_field = params.remove("sort");
        let order = params.remove("order");
        let limit = params
            .remove("limit")
            .and_then(|s| s.parse::<usize>().ok());
        let offset = params
            .remove("offset")
            .and_then(|s| s.parse::<usize>().ok());

        // Build components
        let filters = Filter::from_params(params);
        let sort = Sort::from_params(sort_field, order);
        let pagination = Pagination::from_params(limit, offset);

        Self {
            filters,
            sort,
            pagination,
        }
    }

    /// Check if there are any filters
    pub fn has_filters(&self) -> bool {
        !self.filters.is_empty()
    }

    /// Check if there is a sort
    pub fn has_sort(&self) -> bool {
        self.sort.is_some()
    }

    /// Get filter by field name
    pub fn get_filter(&self, field: &str) -> Option<&Filter> {
        self.filters.iter().find(|f| f.field == field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilterValue, SortOrder};

    #[test]
    fn test_default_query_params() {
        let params = QueryParams::default();
        assert_eq!(params.filters.len(), 0);
        assert!(params.sort.is_none());
        assert_eq!(params.pagination.limit, 50);
        assert_eq!(params.pagination.offset, 0);
    }

    #[test]
    fn test_from_query_string() {
        let query = "name=John&age=25&sort=name&order=asc&limit=100&offset=50";
        let params = QueryParams::from_query_string(query).unwrap();

        assert_eq!(params.filters.len(), 2);
        assert!(params.has_filters());

        assert!(params.sort.is_some());
        assert!(params.has_sort());
        let sort = params.sort.unwrap();
        assert_eq!(sort.field, "name");
        assert_eq!(sort.order, SortOrder::Asc);

        assert_eq!(params.pagination.limit, 100);
        assert_eq!(params.pagination.offset, 50);
    }

    #[test]
    fn test_from_query_string_minimal() {
        let query = "name=John";
        let params = QueryParams::from_query_string(query).unwrap();

        assert_eq!(params.filters.len(), 1);
        assert!(params.sort.is_none());
        assert_eq!(params.pagination.limit, 50);
        assert_eq!(params.pagination.offset, 0);
    }

    #[test]
    fn test_from_query_string_empty() {
        let query = "";
        let params = QueryParams::from_query_string(query).unwrap();

        assert_eq!(params.filters.len(), 0);
        assert!(params.sort.is_none());
        assert_eq!(params.pagination.limit, 50);
    }

    #[test]
    fn test_from_map() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "John".to_string());
        params.insert("active".to_string(), "true".to_string());
        params.insert("sort".to_string(), "created_at".to_string());
        params.insert("order".to_string(), "desc".to_string());
        params.insert("limit".to_string(), "20".to_string());
        params.insert("offset".to_string(), "10".to_string());

        let qp = QueryParams::from_map(params);

        assert_eq!(qp.filters.len(), 2);

        let name_filter = qp.get_filter("name");
        assert!(name_filter.is_some());
        let name_filter = name_filter.unwrap();
        assert_eq!(name_filter.value, FilterValue::String("John".to_string()));

        assert!(qp.sort.is_some());
        let sort = qp.sort.as_ref().unwrap();
        assert_eq!(sort.field, "created_at");
        assert_eq!(sort.order, SortOrder::Desc);

        assert_eq!(qp.pagination.limit, 20);
        assert_eq!(qp.pagination.offset, 10);
    }

    #[test]
    fn test_get_filter() {
        let params = QueryParams::from_query_string("name=John&age=25").unwrap();

        let name = params.get_filter("name");
        assert!(name.is_some());

        let missing = params.get_filter("email");
        assert!(missing.is_none());
    }

    #[test]
    fn test_has_filters_and_sort() {
        let empty = QueryParams::default();
        assert!(!empty.has_filters());
        assert!(!empty.has_sort());

        let with_filter = QueryParams::from_query_string("name=John").unwrap();
        assert!(with_filter.has_filters());
        assert!(!with_filter.has_sort());

        let with_sort = QueryParams::from_query_string("sort=name").unwrap();
        assert!(!with_sort.has_filters());
        assert!(with_sort.has_sort());
    }
}
