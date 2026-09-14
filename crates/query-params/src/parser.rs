use crate::{Filter, Pagination, Sort};
use serde::Deserialize;
use std::collections::HashMap;

#[doc = include_str!("../docs/parser.QueryParams.md")]
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QueryParams {
    #[doc = include_str!("../docs/parser.QueryParams.filters.md")]
    #[serde(skip)]
    pub filters: Vec<Filter>,

    #[doc = include_str!("../docs/parser.QueryParams.sort.md")]
    #[serde(skip)]
    pub sort: Option<Sort>,

    #[doc = include_str!("../docs/parser.QueryParams.pagination.md")]
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
    #[doc = include_str!("../docs/parser.QueryParams.new.md")]
    pub fn new(filters: Vec<Filter>, sort: Option<Sort>, pagination: Pagination) -> Self {
        Self {
            filters,
            sort,
            pagination,
        }
    }

    #[doc = include_str!("../docs/parser.QueryParams.from_query_string.md")]
    pub fn from_query_string(query: &str) -> Result<Self, serde_urlencoded::de::Error> {
        let params: HashMap<String, String> = serde_urlencoded::from_str(query)?;
        Ok(Self::from_map(params))
    }

    #[doc = include_str!("../docs/parser.QueryParams.from_map.md")]
    pub fn from_map(mut params: HashMap<String, String>) -> Self {
        let sort_field = params.remove("sort");
        let order = params.remove("order");
        let limit = params.remove("limit").and_then(|s| s.parse::<usize>().ok());
        let offset = params
            .remove("offset")
            .and_then(|s| s.parse::<usize>().ok());

        let filters = Filter::from_params(params);
        let sort = Sort::from_params(sort_field, order);
        let pagination = Pagination::from_params(limit, offset);

        Self {
            filters,
            sort,
            pagination,
        }
    }

    #[doc = include_str!("../docs/parser.QueryParams.has_filters.md")]
    pub fn has_filters(&self) -> bool {
        !self.filters.is_empty()
    }

    #[doc = include_str!("../docs/parser.QueryParams.has_sort.md")]
    pub fn has_sort(&self) -> bool {
        self.sort.is_some()
    }

    #[doc = include_str!("../docs/parser.QueryParams.get_filter.md")]
    pub fn get_filter(&self, field: &str) -> Option<&Filter> {
        self.filters.iter().find(|f| f.field == field)
    }
}
