use serde::Deserialize;
use std::collections::HashMap;

#[doc = include_str!("../docs/filter.FilterValue.md")]
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    #[doc = include_str!("../docs/filter.FilterValue.String.md")]
    String(String),
    #[doc = include_str!("../docs/filter.FilterValue.Number.md")]
    Number(f64),
    #[doc = include_str!("../docs/filter.FilterValue.Bool.md")]
    Bool(bool),
}

#[doc = include_str!("../docs/filter.Filter.md")]
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    #[doc = include_str!("../docs/filter.Filter.field.md")]
    pub field: String,
    #[doc = include_str!("../docs/filter.Filter.value.md")]
    pub value: FilterValue,
}

impl Filter {
    #[doc = include_str!("../docs/filter.Filter.new.md")]
    pub fn new(field: impl Into<String>, value: FilterValue) -> Self {
        Self {
            field: field.into(),
            value,
        }
    }

    #[doc = include_str!("../docs/filter.Filter.from_params.md")]
    pub fn from_params(params: HashMap<String, String>) -> Vec<Filter> {
        params
            .into_iter()
            .filter_map(|(field, value_str)| {
                if matches!(field.as_str(), "sort" | "order" | "limit" | "offset") {
                    return None;
                }

                let value = if let Ok(num) = value_str.parse::<f64>() {
                    if num.is_finite() && num.to_string() == value_str {
                        FilterValue::Number(num)
                    } else {
                        FilterValue::String(value_str)
                    }
                } else if let Ok(b) = value_str.parse::<bool>() {
                    FilterValue::Bool(b)
                } else {
                    FilterValue::String(value_str)
                };

                Some(Filter { field, value })
            })
            .collect()
    }

    #[doc = include_str!("../docs/filter.Filter.matches_string.md")]
    pub fn matches_string(&self, value: &str) -> bool {
        match &self.value {
            FilterValue::String(s) => s == value,
            _ => false,
        }
    }

    #[doc = include_str!("../docs/filter.Filter.matches_number.md")]
    pub fn matches_number(&self, value: f64) -> bool {
        match &self.value {
            FilterValue::Number(n) => *n == value,
            _ => false,
        }
    }

    #[doc = include_str!("../docs/filter.Filter.matches_bool.md")]
    pub fn matches_bool(&self, value: bool) -> bool {
        match &self.value {
            FilterValue::Bool(b) => *b == value,
            _ => false,
        }
    }
}
