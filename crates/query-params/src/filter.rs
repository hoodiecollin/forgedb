//! Filter parameter handling

use serde::Deserialize;
use std::collections::HashMap;

/// Filter value types
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    String(String),
    Number(f64),
    Bool(bool),
}

/// Filter represents a field-value pair for filtering
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub field: String,
    pub value: FilterValue,
}

impl Filter {
    /// Create a new filter
    pub fn new(field: impl Into<String>, value: FilterValue) -> Self {
        Self {
            field: field.into(),
            value,
        }
    }

    /// Parse filters from a HashMap (typically from query string)
    pub fn from_params(params: HashMap<String, String>) -> Vec<Filter> {
        params
            .into_iter()
            .filter_map(|(field, value_str)| {
                // Skip special parameters
                if matches!(field.as_str(), "sort" | "order" | "limit" | "offset") {
                    return None;
                }

                // Try to parse as different types.
                // Only coerce to Number when the round-trip string form matches the input:
                // this rejects leading-zero strings ("01234"), "inf", "nan", and scientific
                // notation ("1e5") that would silently corrupt string identity comparisons.
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

    /// Check if this filter matches a string value
    pub fn matches_string(&self, value: &str) -> bool {
        match &self.value {
            FilterValue::String(s) => s == value,
            _ => false,
        }
    }

    /// Check if this filter matches a number value.
    ///
    /// Uses exact equality. REST filter params represent discrete user-supplied values
    /// (e.g. `age=30`), where exact bit-level equality is the correct semantic.
    /// The previous relative-epsilon comparison was wrong for large magnitudes and
    /// added false positives for nearby-but-distinct values.
    pub fn matches_number(&self, value: f64) -> bool {
        match &self.value {
            FilterValue::Number(n) => *n == value,
            _ => false,
        }
    }

    /// Check if this filter matches a bool value
    pub fn matches_bool(&self, value: bool) -> bool {
        match &self.value {
            FilterValue::Bool(b) => *b == value,
            _ => false,
        }
    }
}

