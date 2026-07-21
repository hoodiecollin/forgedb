use forgedb_query_params::{Filter, FilterValue};
use std::collections::HashMap;

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
