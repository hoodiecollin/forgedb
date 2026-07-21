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
        let limit = params.remove("limit").and_then(|s| s.parse::<usize>().ok());
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
