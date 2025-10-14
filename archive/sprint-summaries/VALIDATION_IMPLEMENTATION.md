# Sprint 2: Validation Implementation

## Overview

Implemented comprehensive schema validation for Sprint 2, as specified in the SPRINT_PLAN.md. The validation system enforces naming conventions, detects duplicates, and provides helpful error messages with line numbers and suggestions.

## Features Implemented

### 1. Field Name Validation (snake_case enforcement)
- Fields must follow snake_case naming convention
- Examples: `user_name`, `email`, `age_123` ✓
- Invalid: `UserName`, `userName`, `User_name` ✗
- Provides suggestions for correct naming

### 2. Model Name Validation (PascalCase enforcement)
- Models must follow PascalCase naming convention
- Examples: `User`, `UserModel`, `Post` ✓
- Invalid: `user`, `user_model`, `User_Name` ✗
- Provides suggestions for correct naming

### 3. Duplicate Field Detection
- Already implemented in parser
- Enhanced with better error messages
- Shows location of duplicate

### 4. Enhanced Error Messages
- All validation errors include line and column numbers
- Provides actionable suggestions for fixes
- Clear, user-friendly error format

## Code Structure

### New Crate: `forgedb-validation`
Location: `crates/validation/`

**Main Components:**

1. **Position Tracking** (`Position` struct)
   - Tracks line and column numbers
   - Used throughout lexer and parser
   - Shared across all validation functions

2. **Validation Functions:**
   - `validate_field_name()` - Enforces snake_case
   - `validate_model_name()` - Enforces PascalCase
   - `check_duplicate_fields()` - Detects duplicate field names
   - `check_duplicate_models()` - Detects duplicate model names

3. **Helper Functions:**
   - `is_snake_case()` - Check if string is snake_case
   - `is_pascal_case()` - Check if string is PascalCase
   - `to_snake_case()` - Convert string to snake_case (for suggestions)
   - `to_pascal_case()` - Convert string to PascalCase (for suggestions)

4. **Error Handling:**
   - `ValidationError` - Rich error type with position and suggestion
   - `ValidationResult<T>` - Type alias for Result with ValidationError
   - Implements Display trait for user-friendly formatting

### Integration with Parser

**Modified Files:**

1. **`src/lexer.rs`**
   - Added `TokenWithPos` struct to track token positions
   - Added `tokenize_with_pos()` method
   - Re-exports `Position` from validation crate

2. **`src/parser.rs`**
   - Updated to track positions for all identifiers
   - Validates field names when parsing fields
   - Validates model names when parsing models
   - Added `new_with_validation()` to allow disabling validation (for testing)
   - Enhanced error messages include position information

## Tests

### Unit Tests (16 tests in validation crate)

**Naming Convention Tests:**
- `test_is_snake_case()` - Tests snake_case detection
- `test_is_pascal_case()` - Tests PascalCase detection
- `test_to_snake_case()` - Tests snake_case conversion
- `test_to_pascal_case()` - Tests PascalCase conversion

**Validation Function Tests:**
- `test_validate_field_name_valid()` - Valid field names
- `test_validate_field_name_invalid()` - Invalid field names with suggestions
- `test_validate_field_name_with_position()` - Position tracking
- `test_validate_model_name_valid()` - Valid model names
- `test_validate_model_name_invalid()` - Invalid model names with suggestions
- `test_validate_model_name_with_position()` - Position tracking

**Duplicate Detection Tests:**
- `test_check_duplicate_fields_no_duplicates()` - No duplicates case
- `test_check_duplicate_fields_with_duplicates()` - Duplicate detection
- `test_check_duplicate_models_no_duplicates()` - No duplicates case
- `test_check_duplicate_models_with_duplicates()` - Duplicate detection

**Error Display Tests:**
- `test_validation_error_display_with_position()` - Error formatting with position
- `test_validation_error_display_without_position()` - Error formatting without position

### Integration Tests (5 new tests in parser)

- `test_validation_field_name_snake_case()` - Field validation in parser
- `test_validation_model_name_pascal_case()` - Model validation in parser
- `test_validation_can_be_disabled()` - Optional validation
- `test_validation_error_with_line_numbers()` - Position in errors
- `test_validation_all_valid()` - Valid schema passes

## Example Output

### Invalid Model Name:
```
Error at line 2, column 1: Model name 'user_model' must be in PascalCase
  Suggestion: Consider using 'UserModel'
```

### Invalid Field Name:
```
Error at line 3, column 1: Field name 'UserName' must be in snake_case
  Suggestion: Consider using 'user_name'
```

### Valid Schema:
```
✓ Parsed 2 models successfully
```

## Running Tests

```bash
# Run all validation tests
cargo test -p forgedb-validation

# Run all tests including integration
cargo test

# Run validation demo
cargo run --example test_validation
```

## Test Results

All tests passing:
- 16 unit tests in `forgedb-validation` crate ✓
- 30 tests in main crate (including 5 new validation tests) ✓
- Total: 46 tests passing ✓

## Usage

### With Validation (default):
```rust
let mut parser = Parser::new(input)?;
let schema = parser.parse()?; // Validates naming conventions
```

### Without Validation (for backwards compatibility):
```rust
let mut parser = Parser::new_with_validation(input, false)?;
let schema = parser.parse()?; // Skips validation
```

## Success Criteria

All Sprint 2 validation requirements met:

- ✅ Validate field names (snake_case enforcement)
- ✅ Validate model names (PascalCase enforcement)
- ✅ Check for duplicate field names
- ✅ Better error messages with line numbers
- ✅ Suggestions for fixing errors
- ✅ Write validation unit tests
- ✅ All tests passing

## Files Modified/Created

### Created:
- `crates/validation/src/lib.rs` - Validation library
- `crates/validation/Cargo.toml` - Validation crate config
- `examples/test_validation.rs` - Validation demo
- `test_schema_invalid.forge` - Test file with invalid schema
- `VALIDATION_IMPLEMENTATION.md` - This document

### Modified:
- `src/lexer.rs` - Added position tracking
- `src/parser.rs` - Integrated validation
- `Cargo.toml` - Added validation dependency and example

## Future Improvements

1. **More Validation Rules:**
   - Reserved keyword checking
   - Type compatibility validation
   - Relationship validation

2. **Better Error Recovery:**
   - Collect multiple errors instead of stopping at first
   - Show all validation errors in one pass

3. **Warning System:**
   - Non-fatal warnings for style issues
   - Deprecation warnings

4. **Configuration:**
   - Allow customization of naming conventions
   - Enable/disable specific validation rules
   - Configuration file support

## Notes

The validation system is designed to be:
- **Extensible**: Easy to add new validation rules
- **Helpful**: Provides suggestions, not just errors
- **Optional**: Can be disabled for testing or backwards compatibility
- **Precise**: Shows exact location of errors with line and column numbers
