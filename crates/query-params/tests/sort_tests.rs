use forgedb_query_params::*;

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
