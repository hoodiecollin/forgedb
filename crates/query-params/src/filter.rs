use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    String(String),
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub field: String,
    pub value: FilterValue,
}

impl Filter {
    pub fn new(field: impl Into<String>, value: FilterValue) -> Self {
        Self {
            field: field.into(),
            value,
        }
    }

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

    pub fn matches_string(&self, value: &str) -> bool {
        match &self.value {
            FilterValue::String(s) => s == value,
            _ => false,
        }
    }

    pub fn matches_number(&self, value: f64) -> bool {
        match &self.value {
            FilterValue::Number(n) => *n == value,
            _ => false,
        }
    }

    pub fn matches_bool(&self, value: bool) -> bool {
        match &self.value {
            FilterValue::Bool(b) => *b == value,
            _ => false,
        }
    }
}
