// Integration tests for Sprint 2
// Tests validation + types + storage working together

#[cfg(test)]
mod fulltext_search_tests;

#[cfg(test)]
mod integration_tests {
    use sinkdb_validation::{validate_field_name, validate_model_name, Position};

    // Integration Test 1: Validation + All 9 type names
    #[test]
    fn test_validation_with_all_type_names() {
        // All type keywords should be valid field names in snake_case context
        let type_names = vec![
            "u32_field", "u64_field", "i32_field", "i64_field",
            "f64_field", "bool_field", "string_field",
            "uuid_field", "timestamp_field"
        ];

        for name in type_names {
            let result = validate_field_name(name, None);
            assert!(result.is_ok(), "Type name '{}' should be valid field name", name);
        }

        // Model names should use PascalCase
        let model_names = vec![
            "U32Model", "U64Model", "I32Model", "I64Model",
            "F64Model", "BoolModel", "StringModel",
            "UuidModel", "TimestampModel"
        ];

        for name in model_names {
            let result = validate_model_name(name, None);
            assert!(result.is_ok(), "Type name '{}' should be valid model name", name);
        }
    }

    // Integration Test 2: Validation errors include positions
    #[test]
    fn test_validation_errors_with_position() {
        let pos = Some(Position { line: 5, column: 10 });

        // Invalid field name
        let result = validate_field_name("InvalidName", pos);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("line 5, column 10"));
        assert!(err.to_string().contains("snake_case"));

        // Invalid model name
        let result = validate_model_name("invalid_name", pos);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("line 5, column 10"));
        assert!(err.to_string().contains("PascalCase"));
    }

    // Integration Test 3: Validation with edge case positions
    #[test]
    fn test_validation_boundary_positions() {
        // Test position at start of file
        let pos_start = Some(Position { line: 1, column: 1 });
        let result = validate_field_name("BadName", pos_start);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 1, column 1"));

        // Test very large position numbers
        let pos_large = Some(Position { line: 9999, column: 9999 });
        let result = validate_model_name("bad_name", pos_large);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 9999, column 9999"));
    }

    // Integration Test 4: Field names with numbers validated correctly
    #[test]
    fn test_validation_field_names_with_numbers() {
        let valid_names = vec![
            "field_1", "field_123", "u32_value",
            "i64_count", "f64_price", "bool_1"
        ];

        for name in valid_names {
            let result = validate_field_name(name, None);
            assert!(result.is_ok(), "Field name '{}' should be valid", name);
        }
    }

    // Integration Test 5: Model names with numbers validated correctly
    #[test]
    fn test_validation_model_names_with_numbers() {
        let valid_names = vec![
            "Model1", "Model123", "User2", "Http2Server"
        ];

        for name in valid_names {
            let result = validate_model_name(name, None);
            assert!(result.is_ok(), "Model name '{}' should be valid", name);
        }
    }

    // Integration Test 6: Special character field names
    #[test]
    fn test_validation_underscore_prefixes() {
        let valid_names = vec!["_private", "__internal", "___triple"];

        for name in valid_names {
            let result = validate_field_name(name, None);
            assert!(result.is_ok(), "Field name '{}' with underscores should be valid", name);
        }
    }

    // Integration Test 7: Reject invalid naming patterns
    #[test]
    fn test_validation_rejects_invalid_patterns() {
        // camelCase fields should be rejected
        let invalid_fields = vec!["userName", "emailAddress", "firstName"];
        for name in invalid_fields {
            let result = validate_field_name(name, None);
            assert!(result.is_err(), "Field name '{}' should be rejected", name);
            assert!(result.unwrap_err().to_string().contains("snake_case"));
        }

        // snake_case models should be rejected
        let invalid_models = vec!["user_model", "email_address", "first_name"];
        for name in invalid_models {
            let result = validate_model_name(name, None);
            assert!(result.is_err(), "Model name '{}' should be rejected", name);
            assert!(result.unwrap_err().to_string().contains("PascalCase"));
        }
    }

    // Integration Test 8: Validation provides helpful suggestions
    #[test]
    fn test_validation_suggestions() {
        // Field name suggestion
        let result = validate_field_name("UserName", None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("user_name"), "Should suggest correct snake_case");

        // Model name suggestion
        let result = validate_model_name("user_model", None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("UserModel"), "Should suggest correct PascalCase");
    }

    // Integration Test 9: Single character names
    #[test]
    fn test_validation_single_char_names() {
        // Single lowercase letter is valid for fields
        assert!(validate_field_name("x", None).is_ok());
        assert!(validate_field_name("a", None).is_ok());
        assert!(validate_field_name("_", None).is_ok());

        // Single uppercase letter is valid for models
        assert!(validate_model_name("X", None).is_ok());
        assert!(validate_model_name("A", None).is_ok());
    }

    // Integration Test 10: Very long names
    #[test]
    fn test_validation_long_names() {
        // Long field name (snake_case)
        let long_field = "this_is_a_very_long_field_name_that_should_still_be_valid_in_snake_case_format";
        assert!(validate_field_name(long_field, None).is_ok());

        // Long model name (PascalCase)
        let long_model = "ThisIsAVeryLongModelNameThatShouldStillBeValidInPascalCaseFormat";
        assert!(validate_model_name(long_model, None).is_ok());
    }
}
