// Schema validation for Sprint 2
// Extended for HTTP validation in Sprint 9

pub mod http;
pub mod status;

use std::collections::HashSet;

pub use http::{HttpValidationError, HttpValidator};
pub use status::StatusCodeMapper;

/// Represents a position in the source code
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Position { line, column }
    }
}

/// Validation error with position information
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub message: String,
    pub position: Option<Position>,
    pub suggestion: Option<String>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        ValidationError {
            message: message.into(),
            position: None,
            suggestion: None,
        }
    }

    pub fn with_position(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pos) = self.position {
            write!(f, "Error at line {}, column {}: {}", pos.line, pos.column, self.message)?;
        } else {
            write!(f, "Error: {}", self.message)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

/// Check if a string is in snake_case format
pub fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with lowercase letter or underscore
    if !s.chars().next().unwrap().is_lowercase() && !s.starts_with('_') {
        return false;
    }

    // Can only contain lowercase letters, digits, and underscores
    s.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Check if a string is in PascalCase format
pub fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with uppercase letter
    if !s.chars().next().unwrap().is_uppercase() {
        return false;
    }

    // Can only contain letters and digits, no underscores
    s.chars().all(|c| c.is_alphanumeric()) && !s.contains('_')
}

/// Convert a string to snake_case (for suggestions)
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];

        if c.is_uppercase() {
            // Add underscore before uppercase if:
            // - Not the first character
            // - Previous char was lowercase or digit
            // - OR next char is lowercase (e.g., "HTTPServer" -> "http_server")
            if i > 0 {
                let prev = chars[i - 1];
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

                if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_is_lower) {
                    result.push('_');
                }
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }

    result
}

/// Convert a string to PascalCase (for suggestions)
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Validate field name follows snake_case convention
pub fn validate_field_name(name: &str, position: Option<Position>) -> ValidationResult<()> {
    if !is_snake_case(name) {
        let suggestion = to_snake_case(name);
        let error = ValidationError::new(format!("Field name '{}' must be in snake_case", name))
            .with_suggestion(format!("Consider using '{}'", suggestion));

        if let Some(pos) = position {
            return Err(error.with_position(pos));
        }
        return Err(error);
    }
    Ok(())
}

/// Validate model name follows PascalCase convention
pub fn validate_model_name(name: &str, position: Option<Position>) -> ValidationResult<()> {
    if !is_pascal_case(name) {
        let suggestion = to_pascal_case(name);
        let error = ValidationError::new(format!("Model name '{}' must be in PascalCase", name))
            .with_suggestion(format!("Consider using '{}'", suggestion));

        if let Some(pos) = position {
            return Err(error.with_position(pos));
        }
        return Err(error);
    }
    Ok(())
}

/// Check for duplicate field names in a collection
pub fn check_duplicate_fields(fields: &[(String, Option<Position>)]) -> ValidationResult<()> {
    let mut seen = HashSet::new();

    for (name, position) in fields {
        if seen.contains(name) {
            let error = ValidationError::new(format!("Duplicate field name '{}'", name));
            if let Some(pos) = position {
                return Err(error.with_position(*pos));
            }
            return Err(error);
        }
        seen.insert(name.clone());
    }

    Ok(())
}

/// Check for duplicate model names in a collection
pub fn check_duplicate_models(models: &[(String, Option<Position>)]) -> ValidationResult<()> {
    let mut seen = HashSet::new();

    for (name, position) in models {
        if seen.contains(name) {
            let error = ValidationError::new(format!("Duplicate model name '{}'", name));
            if let Some(pos) = position {
                return Err(error.with_position(*pos));
            }
            return Err(error);
        }
        seen.insert(name.clone());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_snake_case() {
        assert!(is_snake_case("user_name"));
        assert!(is_snake_case("user_name_123"));
        assert!(is_snake_case("user123"));
        assert!(is_snake_case("_private"));
        assert!(is_snake_case("email"));

        assert!(!is_snake_case("UserName"));
        assert!(!is_snake_case("userName"));
        assert!(!is_snake_case("user-name"));
        assert!(!is_snake_case(""));
        assert!(!is_snake_case("User_name"));
    }

    #[test]
    fn test_is_pascal_case() {
        assert!(is_pascal_case("UserName"));
        assert!(is_pascal_case("User"));
        assert!(is_pascal_case("User123"));
        assert!(is_pascal_case("MyModel"));

        assert!(!is_pascal_case("userName"));
        assert!(!is_pascal_case("user_name"));
        assert!(!is_pascal_case("User_Name"));
        assert!(!is_pascal_case(""));
        assert!(!is_pascal_case("user"));
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("UserName"), "user_name");
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("MyModel"), "my_model");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_snake_case("user_name"), "user_name");
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("ID"), "id");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("user_name"), "UserName");
        assert_eq!(to_pascal_case("user"), "User");
        assert_eq!(to_pascal_case("my_model"), "MyModel");
        assert_eq!(to_pascal_case("UserName"), "UserName");
    }

    #[test]
    fn test_validate_field_name_valid() {
        assert!(validate_field_name("user_name", None).is_ok());
        assert!(validate_field_name("email", None).is_ok());
        assert!(validate_field_name("age_123", None).is_ok());
    }

    #[test]
    fn test_validate_field_name_invalid() {
        let result = validate_field_name("UserName", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("snake_case"));
        assert!(err.suggestion.is_some());
        assert!(err.suggestion.unwrap().contains("user_name"));
    }

    #[test]
    fn test_validate_field_name_with_position() {
        let pos = Position::new(10, 5);
        let result = validate_field_name("BadName", Some(pos));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.position, Some(pos));
    }

    #[test]
    fn test_validate_model_name_valid() {
        assert!(validate_model_name("User", None).is_ok());
        assert!(validate_model_name("UserModel", None).is_ok());
        assert!(validate_model_name("MyModel123", None).is_ok());
    }

    #[test]
    fn test_validate_model_name_invalid() {
        let result = validate_model_name("user_model", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("PascalCase"));
        assert!(err.suggestion.is_some());
        assert!(err.suggestion.unwrap().contains("UserModel"));
    }

    #[test]
    fn test_validate_model_name_with_position() {
        let pos = Position::new(5, 1);
        let result = validate_model_name("bad_name", Some(pos));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.position, Some(pos));
    }

    #[test]
    fn test_check_duplicate_fields_no_duplicates() {
        let fields = vec![
            ("email".to_string(), None),
            ("username".to_string(), None),
            ("age".to_string(), None),
        ];
        assert!(check_duplicate_fields(&fields).is_ok());
    }

    #[test]
    fn test_check_duplicate_fields_with_duplicates() {
        let fields = vec![
            ("email".to_string(), None),
            ("username".to_string(), None),
            ("email".to_string(), Some(Position::new(5, 3))),
        ];
        let result = check_duplicate_fields(&fields);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Duplicate field name 'email'"));
        assert_eq!(err.position, Some(Position::new(5, 3)));
    }

    #[test]
    fn test_check_duplicate_models_no_duplicates() {
        let models = vec![
            ("User".to_string(), None),
            ("Post".to_string(), None),
            ("Comment".to_string(), None),
        ];
        assert!(check_duplicate_models(&models).is_ok());
    }

    #[test]
    fn test_check_duplicate_models_with_duplicates() {
        let models = vec![
            ("User".to_string(), None),
            ("Post".to_string(), None),
            ("User".to_string(), Some(Position::new(20, 1))),
        ];
        let result = check_duplicate_models(&models);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Duplicate model name 'User'"));
        assert_eq!(err.position, Some(Position::new(20, 1)));
    }

    #[test]
    fn test_validation_error_display_with_position() {
        let error = ValidationError::new("Test error")
            .with_position(Position::new(10, 5))
            .with_suggestion("Try this instead");

        let display = format!("{}", error);
        assert!(display.contains("line 10"));
        assert!(display.contains("column 5"));
        assert!(display.contains("Test error"));
        assert!(display.contains("Try this instead"));
    }

    #[test]
    fn test_validation_error_display_without_position() {
        let error = ValidationError::new("Test error")
            .with_suggestion("Try this instead");

        let display = format!("{}", error);
        assert!(!display.contains("line"));
        assert!(display.contains("Test error"));
        assert!(display.contains("Try this instead"));
    }

    // Edge case tests

    #[test]
    fn test_snake_case_edge_cases() {
        // Single character
        assert!(is_snake_case("a"));
        assert!(is_snake_case("x"));
        assert!(!is_snake_case("A"));

        // Leading underscore (private fields)
        assert!(is_snake_case("_private"));
        assert!(is_snake_case("__double"));

        // Numbers
        assert!(is_snake_case("field123"));
        assert!(is_snake_case("abc_123_def"));
        assert!(!is_snake_case("123field")); // Can't start with number

        // Multiple underscores
        assert!(is_snake_case("a__b")); // Double underscore is technically valid
        assert!(is_snake_case("___"));

        // Mixed invalid cases
        assert!(!is_snake_case("camelCase"));
        assert!(!is_snake_case("SCREAMING_CASE"));
        assert!(!is_snake_case("kebab-case"));
        assert!(!is_snake_case("dot.case"));
    }

    #[test]
    fn test_pascal_case_edge_cases() {
        // Single character
        assert!(is_pascal_case("A"));
        assert!(is_pascal_case("X"));
        assert!(!is_pascal_case("a"));

        // Numbers
        assert!(is_pascal_case("Model123"));
        assert!(is_pascal_case("HTTP2Server"));
        assert!(!is_pascal_case("123Model")); // Can't start with number

        // Invalid cases
        assert!(!is_pascal_case("snake_case"));
        assert!(is_pascal_case("SCREAMING")); // All caps is technically valid (no underscores, starts with uppercase)
        assert!(!is_pascal_case("kebab-case"));
        assert!(!is_pascal_case("dot.case"));
        assert!(!is_pascal_case("camelCase"));
    }

    #[test]
    fn test_to_snake_case_edge_cases() {
        // Already snake_case
        assert_eq!(to_snake_case("already_snake"), "already_snake");

        // Single char
        assert_eq!(to_snake_case("A"), "a");

        // Consecutive capitals (acronyms)
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("HTMLElement"), "html_element");
        assert_eq!(to_snake_case("HTTPAPI"), "httpapi"); // All caps -> lowercase

        // Numbers
        assert_eq!(to_snake_case("User123"), "user123");
        assert_eq!(to_snake_case("HTML5Parser"), "html5_parser");

        // camelCase
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("camelCaseExample"), "camel_case_example");

        // Edge: already has underscores (these get preserved, not converted)
        // Note: This is expected behavior - we don't try to fix mixed conventions
        assert_eq!(to_snake_case("User_Name"), "user_name"); // Underscore is preserved
    }

    #[test]
    fn test_to_pascal_case_edge_cases() {
        // Already PascalCase
        assert_eq!(to_pascal_case("AlreadyPascal"), "AlreadyPascal");

        // Single char
        assert_eq!(to_pascal_case("a"), "A");

        // Multiple underscores
        assert_eq!(to_pascal_case("a__b"), "AB");
        assert_eq!(to_pascal_case("___test"), "Test");

        // Leading underscore
        assert_eq!(to_pascal_case("_private"), "Private");

        // Numbers
        assert_eq!(to_pascal_case("field_123"), "Field123");

        // Empty sections
        assert_eq!(to_pascal_case("_"), "");
    }

    #[test]
    fn test_duplicate_fields_edge_cases() {
        // Empty list
        let empty: Vec<(String, Option<Position>)> = vec![];
        assert!(check_duplicate_fields(&empty).is_ok());

        // Single field
        let single = vec![("field".to_string(), None)];
        assert!(check_duplicate_fields(&single).is_ok());

        // Case sensitivity - these should be treated as different
        let case_sensitive = vec![
            ("email".to_string(), None),
            ("Email".to_string(), None),
        ];
        assert!(check_duplicate_fields(&case_sensitive).is_ok());

        // First duplicate should be reported (not second)
        let three_duplicates = vec![
            ("name".to_string(), None),
            ("name".to_string(), Some(Position::new(5, 1))),
            ("name".to_string(), Some(Position::new(10, 1))),
        ];
        let result = check_duplicate_fields(&three_duplicates);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should report the second occurrence (first duplicate)
        assert_eq!(err.position, Some(Position::new(5, 1)));
    }

    #[test]
    fn test_duplicate_models_edge_cases() {
        // Empty list
        let empty: Vec<(String, Option<Position>)> = vec![];
        assert!(check_duplicate_models(&empty).is_ok());

        // Single model
        let single = vec![("User".to_string(), None)];
        assert!(check_duplicate_models(&single).is_ok());

        // Case sensitivity
        let case_sensitive = vec![
            ("User".to_string(), None),
            ("user".to_string(), None),
        ];
        assert!(check_duplicate_models(&case_sensitive).is_ok());
    }

    #[test]
    fn test_validation_error_builder() {
        // Error without position or suggestion
        let err1 = ValidationError::new("Simple error");
        assert_eq!(err1.message, "Simple error");
        assert_eq!(err1.position, None);
        assert_eq!(err1.suggestion, None);

        // Error with only position
        let err2 = ValidationError::new("Error with position")
            .with_position(Position::new(5, 10));
        assert_eq!(err2.position, Some(Position::new(5, 10)));
        assert_eq!(err2.suggestion, None);

        // Error with only suggestion
        let err3 = ValidationError::new("Error with suggestion")
            .with_suggestion("Fix it this way");
        assert_eq!(err3.position, None);
        assert!(err3.suggestion.is_some());

        // Chain both
        let err4 = ValidationError::new("Full error")
            .with_position(Position::new(1, 1))
            .with_suggestion("Try this");
        assert!(err4.position.is_some());
        assert!(err4.suggestion.is_some());
    }

    #[test]
    fn test_validate_edge_case_names() {
        // Single character field names
        assert!(validate_field_name("x", None).is_ok());
        assert!(validate_field_name("a", None).is_ok());
        assert!(validate_field_name("_", None).is_ok());

        // Single character model names
        assert!(validate_model_name("X", None).is_ok());
        assert!(validate_model_name("A", None).is_ok());

        // Very long names (should still work)
        let long_field = "very_long_field_name_with_many_underscores_that_is_still_valid";
        assert!(validate_field_name(long_field, None).is_ok());

        let long_model = "VeryLongModelNameWithManyCamelCaseWordsThatIsStillValid";
        assert!(validate_model_name(long_model, None).is_ok());
    }
}
