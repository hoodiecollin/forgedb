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

                // Try to parse as different types
                let value = if let Ok(num) = value_str.parse::<f64>() {
                    FilterValue::Number(num)
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

    /// Check if this filter matches a number value
    pub fn matches_number(&self, value: f64) -> bool {
        match &self.value {
            FilterValue::Number(n) => (*n - value).abs() < f64::EPSILON,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_creation() {
        let filter = Filter::new("name", FilterValue::String("John".to_string()));
        assert_eq!(filter.field, "name");
        assert_eq!(filter.value, FilterValue::String("John".to_string()));
    }

    #[test]
    fn test_from_params() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "John".to_string());
        params.insert("age".to_string(), "25".to_string());
        params.insert("active".to_string(), "true".to_string());
        params.insert("sort".to_string(), "name".to_string()); // should be skipped

        let filters = Filter::from_params(params);
        assert_eq!(filters.len(), 3); // sort is skipped

        // Find name filter
        let name_filter = filters.iter().find(|f| f.field == "name").unwrap();
        assert_eq!(name_filter.value, FilterValue::String("John".to_string()));

        // Find age filter
        let age_filter = filters.iter().find(|f| f.field == "age").unwrap();
        assert_eq!(age_filter.value, FilterValue::Number(25.0));

        // Find active filter
        let active_filter = filters.iter().find(|f| f.field == "active").unwrap();
        assert_eq!(active_filter.value, FilterValue::Bool(true));
    }

    #[test]
    fn test_matches_string() {
        let filter = Filter::new("name", FilterValue::String("John".to_string()));
        assert!(filter.matches_string("John"));
        assert!(!filter.matches_string("Jane"));
    }

    #[test]
    fn test_matches_number() {
        let filter = Filter::new("age", FilterValue::Number(25.0));
        assert!(filter.matches_number(25.0));
        assert!(!filter.matches_number(26.0));
    }

    #[test]
    fn test_matches_bool() {
        let filter = Filter::new("active", FilterValue::Bool(true));
        assert!(filter.matches_bool(true));
        assert!(!filter.matches_bool(false));
    }
}
