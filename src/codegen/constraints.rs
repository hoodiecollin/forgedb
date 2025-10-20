//! Centralized constraint mapping
//!
//! Maps DSL constraints (min, max, pattern, email, url, etc.) to various targets:
//! - OpenAPI schema attributes
//! - Rust request validation
//! - TypeScript doc comments and hints

use crate::ast::{Constraint, ConstraintParam, FieldType};
use serde_json::{json, Map, Value};

/// Constraint information for code generation
#[derive(Debug, Clone)]
pub struct ConstraintInfo {
    /// The original constraint from AST
    pub constraint: Constraint,
    
    /// OpenAPI schema properties to add
    pub openapi_props: Map<String, Value>,
    
    /// Rust validation expression (if applicable)
    pub rust_validation: Option<String>,
    
    /// TypeScript hint/comment
    pub ts_hint: Option<String>,
}

/// Map a constraint to its representations for different targets
pub fn map_constraint(constraint: &Constraint, field_type: &FieldType) -> ConstraintInfo {
    let mut openapi_props = Map::new();
    let rust_validation: Option<String>;
    let ts_hint: Option<String>;

    match constraint.name.as_str() {
        "min" => {
            if let Some(ConstraintParam::Number(n)) = constraint.params.first() {
                match field_type {
                    FieldType::String => {
                        openapi_props.insert("minLength".to_string(), json!(n));
                        rust_validation = Some(format!(
                            "value.len() >= {} as usize",
                            n
                        ));
                        ts_hint = Some(format!("Minimum length: {}", n));
                    }
                    FieldType::I32 | FieldType::I64 | FieldType::U32 | FieldType::U64 | FieldType::F64 => {
                        openapi_props.insert("minimum".to_string(), json!(n));
                        rust_validation = Some(format!("*value >= {}", n));
                        ts_hint = Some(format!("Minimum value: {}", n));
                    }
                    _ => {
                        rust_validation = None;
                        ts_hint = None;
                    }
                }
            } else {
                rust_validation = None;
                ts_hint = None;
            }
        }
        "max" => {
            if let Some(ConstraintParam::Number(n)) = constraint.params.first() {
                match field_type {
                    FieldType::String => {
                        openapi_props.insert("maxLength".to_string(), json!(n));
                        rust_validation = Some(format!(
                            "value.len() <= {} as usize",
                            n
                        ));
                        ts_hint = Some(format!("Maximum length: {}", n));
                    }
                    FieldType::I32 | FieldType::I64 | FieldType::U32 | FieldType::U64 | FieldType::F64 => {
                        openapi_props.insert("maximum".to_string(), json!(n));
                        rust_validation = Some(format!("*value <= {}", n));
                        ts_hint = Some(format!("Maximum value: {}", n));
                    }
                    _ => {
                        rust_validation = None;
                        ts_hint = None;
                    }
                }
            } else {
                rust_validation = None;
                ts_hint = None;
            }
        }
        "pattern" => {
            if let Some(ConstraintParam::String(pattern)) = constraint.params.first() {
                openapi_props.insert("pattern".to_string(), json!(pattern));
                rust_validation = Some(format!(
                    "regex::Regex::new(r\"{}\").unwrap().is_match(value)",
                    pattern.replace('\\', "\\\\").replace('"', "\\\"")
                ));
                ts_hint = Some(format!("Pattern: {}", pattern));
            } else {
                rust_validation = None;
                ts_hint = None;
            }
        }
        "email" => {
            openapi_props.insert("format".to_string(), json!("email"));
            rust_validation = Some(
                "value.contains('@') && value.split('@').count() == 2".to_string()
            );
            ts_hint = Some("Must be a valid email address".to_string());
        }
        "url" => {
            openapi_props.insert("format".to_string(), json!("uri"));
            rust_validation = Some(
                "value.starts_with(\"http://\") || value.starts_with(\"https://\")".to_string()
            );
            ts_hint = Some("Must be a valid URL".to_string());
        }
        "uuid" => {
            openapi_props.insert("format".to_string(), json!("uuid"));
            rust_validation = Some(
                "uuid::Uuid::parse_str(value).is_ok()".to_string()
            );
            ts_hint = Some("Must be a valid UUID".to_string());
        }
        "date" => {
            openapi_props.insert("format".to_string(), json!("date"));
            rust_validation = None; // Date validation could be more complex
            ts_hint = Some("Must be a valid date (YYYY-MM-DD)".to_string());
        }
        "datetime" => {
            openapi_props.insert("format".to_string(), json!("date-time"));
            rust_validation = None; // DateTime validation could be more complex
            ts_hint = Some("Must be a valid date-time (ISO 8601)".to_string());
        }
        _ => {
            rust_validation = None;
            ts_hint = None;
        }
    }

    ConstraintInfo {
        constraint: constraint.clone(),
        openapi_props,
        rust_validation,
        ts_hint,
    }
}

/// Get all OpenAPI properties for a list of constraints
pub fn get_openapi_properties(constraints: &[Constraint], field_type: &FieldType) -> Map<String, Value> {
    let mut props = Map::new();
    
    for constraint in constraints {
        let info = map_constraint(constraint, field_type);
        for (key, value) in info.openapi_props {
            props.insert(key, value);
        }
    }
    
    props
}

/// Generate validation expressions for Rust
pub fn get_rust_validations(constraints: &[Constraint], field_type: &FieldType) -> Vec<String> {
    constraints
        .iter()
        .filter_map(|c| {
            let info = map_constraint(c, field_type);
            info.rust_validation
        })
        .collect()
}

/// Get TypeScript hints for constraints
pub fn get_typescript_hints(constraints: &[Constraint], field_type: &FieldType) -> Vec<String> {
    constraints
        .iter()
        .filter_map(|c| {
            let info = map_constraint(c, field_type);
            info.ts_hint
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_constraint(name: &str, params: Vec<ConstraintParam>) -> Constraint {
        Constraint {
            name: name.to_string(),
            params,
        }
    }

    #[test]
    fn test_min_constraint_string() {
        let constraint = create_constraint("min", vec![ConstraintParam::Number(5)]);
        let info = map_constraint(&constraint, &FieldType::String);
        
        assert_eq!(info.openapi_props.get("minLength"), Some(&json!(5)));
        assert!(info.rust_validation.is_some());
        assert!(info.ts_hint.is_some());
    }

    #[test]
    fn test_max_constraint_number() {
        let constraint = create_constraint("max", vec![ConstraintParam::Number(100)]);
        let info = map_constraint(&constraint, &FieldType::I32);
        
        assert_eq!(info.openapi_props.get("maximum"), Some(&json!(100)));
        assert!(info.rust_validation.is_some());
    }

    #[test]
    fn test_pattern_constraint() {
        let constraint = create_constraint("pattern", vec![ConstraintParam::String("^[A-Z]+$".to_string())]);
        let info = map_constraint(&constraint, &FieldType::String);
        
        assert_eq!(info.openapi_props.get("pattern"), Some(&json!("^[A-Z]+$")));
        assert!(info.rust_validation.is_some());
    }

    #[test]
    fn test_email_constraint() {
        let constraint = create_constraint("email", vec![]);
        let info = map_constraint(&constraint, &FieldType::String);
        
        assert_eq!(info.openapi_props.get("format"), Some(&json!("email")));
        assert!(info.rust_validation.is_some());
        assert!(info.ts_hint.is_some());
    }

    #[test]
    fn test_url_constraint() {
        let constraint = create_constraint("url", vec![]);
        let info = map_constraint(&constraint, &FieldType::String);
        
        assert_eq!(info.openapi_props.get("format"), Some(&json!("uri")));
        assert!(info.rust_validation.is_some());
    }

    #[test]
    fn test_get_openapi_properties() {
        let constraints = vec![
            create_constraint("min", vec![ConstraintParam::Number(1)]),
            create_constraint("max", vec![ConstraintParam::Number(100)]),
        ];
        let props = get_openapi_properties(&constraints, &FieldType::I32);
        
        assert_eq!(props.get("minimum"), Some(&json!(1)));
        assert_eq!(props.get("maximum"), Some(&json!(100)));
    }

    #[test]
    fn test_get_rust_validations() {
        let constraints = vec![
            create_constraint("min", vec![ConstraintParam::Number(1)]),
            create_constraint("email", vec![]),
        ];
        let validations = get_rust_validations(&constraints, &FieldType::String);
        
        assert_eq!(validations.len(), 2);
    }

    #[test]
    fn test_get_typescript_hints() {
        let constraints = vec![
            create_constraint("min", vec![ConstraintParam::Number(5)]),
            create_constraint("email", vec![]),
        ];
        let hints = get_typescript_hints(&constraints, &FieldType::String);
        
        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("Minimum length: 5"));
        assert!(hints[1].contains("email"));
    }
}
