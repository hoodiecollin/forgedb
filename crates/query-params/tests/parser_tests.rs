use forgedb_query_params::*;
use std::collections::HashMap;

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
