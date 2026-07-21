# forgedb-validation

Schema and HTTP validation for ForgeDB with position tracking, error reporting, and status code mapping.

## Overview

The `forgedb-validation` crate provides validation capabilities for ForgeDB schemas and HTTP requests/responses. It includes:

- **Schema validation** - Naming conventions, duplicate checks, constraint validation
- **HTTP validation** - Request field validation, format checking, range validation
- **Status code mapping** - Automatic HTTP status code assignment for validation errors
- **Error reporting** - Rich error messages with position tracking and suggestions

## Features

- **Schema validation rules** - Enforce snake_case for fields, PascalCase for models
- **Constraint checking** - Validate required fields, duplicates, type formats
- **Position tracking** - Track errors with line and column information
- **Automatic suggestions** - Provide helpful suggestions for fixing errors
- **HTTP validation** - Email, length, range, and required field validation
- **Status code mapping** - Consistent HTTP status codes for different error types
- **Error formatting** - Clear, actionable error messages with suggestions
- **Zero dependencies** - Minimal, focused crate with no external dependencies

## Schema Validation

### Naming Conventions

ForgeDB enforces consistent naming conventions to ensure clean, readable schemas:

#### Field Names (snake_case)

Fields must use snake_case: lowercase letters, digits, and underscores only.

```rust
use forgedb_validation::{validate_field_name, Position};

// Valid field names
assert!(validate_field_name("user_name", None).is_ok());
assert!(validate_field_name("email", None).is_ok());
assert!(validate_field_name("age_123", None).is_ok());
assert!(validate_field_name("_private", None).is_ok());

// Invalid field names - get helpful suggestions
let result = validate_field_name("UserName", None);
assert!(result.is_err());
// Error: "Field name 'UserName' must be in snake_case"
// Suggestion: "Consider using 'user_name'"
```

#### Model Names (PascalCase)

Models must use PascalCase: start with uppercase, no underscores.

```rust
use forgedb_validation::validate_model_name;

// Valid model names
assert!(validate_model_name("User", None).is_ok());
assert!(validate_model_name("UserModel", None).is_ok());
assert!(validate_model_name("MyModel123", None).is_ok());

// Invalid model names - get helpful suggestions
let result = validate_model_name("user_model", None);
assert!(result.is_err());
// Error: "Model name 'user_model' must be in PascalCase"
// Suggestion: "Consider using 'UserModel'"
```

### Position Tracking

Validation errors can track exact source code positions for precise error reporting:

```rust
use forgedb_validation::{validate_field_name, Position};

let pos = Position::new(10, 5); // line 10, column 5
let result = validate_field_name("BadName", Some(pos));

match result {
    Err(err) => {
        assert_eq!(err.position, Some(pos));
        println!("{}", err);
        // Output: "Error at line 10, column 5: Field name 'BadName' must be in snake_case"
        //         "  Suggestion: Consider using 'bad_name'"
    }
    Ok(_) => {}
}
```

### Duplicate Checking

Prevent duplicate field and model names in schemas:

```rust
use forgedb_validation::{check_duplicate_fields, check_duplicate_models, Position};

// Check for duplicate fields
let fields = vec![
    ("email".to_string(), Some(Position::new(5, 1))),
    ("username".to_string(), Some(Position::new(6, 1))),
    ("email".to_string(), Some(Position::new(10, 1))), // Duplicate!
];

let result = check_duplicate_fields(&fields);
assert!(result.is_err());
// Error at line 10: "Duplicate field name 'email'"

// Check for duplicate models
let models = vec![
    ("User".to_string(), None),
    ("Post".to_string(), None),
    ("User".to_string(), Some(Position::new(20, 1))), // Duplicate!
];

let result = check_duplicate_models(&models);
assert!(result.is_err());
// Error at line 20: "Duplicate model name 'User'"
```

### Case Conversion Utilities

Convert between naming conventions for suggestions and code generation:

```rust
use forgedb_validation::{to_snake_case, to_pascal_case};

// Convert to snake_case
assert_eq!(to_snake_case("UserName"), "user_name");
assert_eq!(to_snake_case("HTTPServer"), "http_server");
assert_eq!(to_snake_case("userId"), "user_id");

// Convert to PascalCase
assert_eq!(to_pascal_case("user_name"), "UserName");
assert_eq!(to_pascal_case("http_server"), "HttpServer");
```

## HTTP Validation

### Request Validation

Validate HTTP request data with built-in validators:

#### Required Fields

```rust
use forgedb_validation::HttpValidator;

let fields = vec![
    ("name", Some("John Doe")),
    ("email", Some("john@example.com")),
    ("phone", None), // Missing!
];

match HttpValidator::validate_required_fields(&fields) {
    Ok(_) => println!("All required fields present"),
    Err(errors) => {
        for error in errors {
            println!("{}", error.message);
            // Output: "Field 'phone' is required"
        }
    }
}
```

#### Email Validation

```rust
use forgedb_validation::HttpValidator;

// Valid emails
assert!(HttpValidator::validate_email("user@example.com").is_ok());
assert!(HttpValidator::validate_email("test@domain.co.uk").is_ok());

// Invalid emails - get helpful suggestions
let result = HttpValidator::validate_email("invalid");
assert!(result.is_err());
// Error: "Invalid email format"
// Suggestion: "Email must contain @ and domain"
```

#### String Length Validation

```rust
use forgedb_validation::HttpValidator;

// Validate string length (min: 2, max: 10)
assert!(HttpValidator::validate_length("name", "John", 2, 10).is_ok());

// Too short
let result = HttpValidator::validate_length("name", "J", 2, 10);
assert!(result.is_err());
// Error: "Field 'name' must be at least 2 characters"

// Too long
let result = HttpValidator::validate_length("name", "VeryLongName", 2, 10);
assert!(result.is_err());
// Error: "Field 'name' must be at most 10 characters"
```

#### Numeric Range Validation

```rust
use forgedb_validation::HttpValidator;

// Validate numeric range (min: 0, max: 150)
assert!(HttpValidator::validate_range("age", 25, 0, 150).is_ok());

// Below minimum
let result = HttpValidator::validate_range("age", -1, 0, 150);
assert!(result.is_err());
// Error: "Field 'age' must be at least 0"

// Above maximum
let result = HttpValidator::validate_range("age", 200, 0, 150);
assert!(result.is_err());
// Error: "Field 'age' must be at most 150"
```

### HTTP Validation Errors

Map validation errors to appropriate HTTP status codes:

```rust
use forgedb_validation::{HttpValidationError, ValidationError};

// Bad Request (400) - Invalid input
let error = HttpValidationError::bad_request(vec![
    ValidationError::new("Field 'email' is required"),
    ValidationError::new("Field 'age' must be at least 0"),
]);
assert_eq!(error.status_code, 400);
assert!(error.is_client_error());

// Not Found (404)
let error = HttpValidationError::not_found("User not found");
assert_eq!(error.status_code, 404);

// Conflict (409) - Uniqueness violation
let error = HttpValidationError::conflict("Email already exists");
assert_eq!(error.status_code, 409);

// Unprocessable Entity (422) - Business logic validation
let error = HttpValidationError::unprocessable_entity(vec![
    ValidationError::new("Cannot delete user with active orders"),
]);
assert_eq!(error.status_code, 422);

// Internal Server Error (500)
let error = HttpValidationError::internal_error("Database connection failed");
assert_eq!(error.status_code, 500);
assert!(error.is_server_error());
```

## Status Code Mapping

The `StatusCodeMapper` provides consistent HTTP status codes for different validation error types:

### Error Type to Status Code

```rust
use forgedb_validation::StatusCodeMapper;

// Map validation error types to status codes
assert_eq!(StatusCodeMapper::for_validation_error("required_field"), 400);
assert_eq!(StatusCodeMapper::for_validation_error("invalid_format"), 400);
assert_eq!(StatusCodeMapper::for_validation_error("not_found"), 404);
assert_eq!(StatusCodeMapper::for_validation_error("already_exists"), 409);
assert_eq!(StatusCodeMapper::for_validation_error("unique_violation"), 409);
assert_eq!(StatusCodeMapper::for_validation_error("foreign_key_violation"), 422);
assert_eq!(StatusCodeMapper::for_validation_error("internal_error"), 500);

// Unknown types default to 400 (Bad Request)
assert_eq!(StatusCodeMapper::for_validation_error("unknown"), 400);
```

### Status Code Utilities

```rust
use forgedb_validation::StatusCodeMapper;

// Get human-readable status names
assert_eq!(StatusCodeMapper::status_name(200), "OK");
assert_eq!(StatusCodeMapper::status_name(404), "Not Found");
assert_eq!(StatusCodeMapper::status_name(500), "Internal Server Error");

// Check status code categories
assert!(StatusCodeMapper::is_success(200));      // 2xx
assert!(StatusCodeMapper::is_client_error(400)); // 4xx
assert!(StatusCodeMapper::is_server_error(500)); // 5xx
```

## Usage Examples

### Complete Schema Validation Example

```rust
use forgedb_validation::{
    ValidationError, ValidationResult, Position,
    validate_model_name, validate_field_name,
    check_duplicate_fields, check_duplicate_models,
};

fn validate_schema() -> ValidationResult<()> {
    // Validate model name
    validate_model_name("User", Some(Position::new(1, 1)))?;
    
    // Define fields with positions
    let fields = vec![
        ("id".to_string(), Some(Position::new(2, 5))),
        ("email".to_string(), Some(Position::new(3, 5))),
        ("username".to_string(), Some(Position::new(4, 5))),
    ];
    
    // Validate field names
    for (name, pos) in &fields {
        validate_field_name(name, *pos)?;
    }
    
    // Check for duplicates
    check_duplicate_fields(&fields)?;
    
    Ok(())
}

match validate_schema() {
    Ok(_) => println!("Schema is valid!"),
    Err(err) => eprintln!("{}", err),
}
```

### Complete HTTP Request Validation Example

```rust
use forgedb_validation::{HttpValidator, HttpValidationError, ValidationError};

fn validate_user_registration(
    email: Option<&str>,
    username: Option<&str>,
    age: Option<i32>,
) -> Result<(), HttpValidationError> {
    let mut errors = Vec::new();
    
    // Validate required fields
    let fields = vec![
        ("email", email),
        ("username", username),
    ];
    
    if let Err(errs) = HttpValidator::validate_required_fields(&fields) {
        errors.extend(errs);
    }
    
    // Validate email format
    if let Some(email_val) = email {
        if let Err(err) = HttpValidator::validate_email(email_val) {
            errors.push(err);
        }
    }
    
    // Validate username length
    if let Some(username_val) = username {
        if let Err(err) = HttpValidator::validate_length("username", username_val, 3, 20) {
            errors.push(err);
        }
    }
    
    // Validate age range
    if let Some(age_val) = age {
        if let Err(err) = HttpValidator::validate_range("age", age_val, 13, 120) {
            errors.push(err);
        }
    }
    
    if errors.is_empty() {
        Ok(())
    } else {
        Err(HttpValidationError::bad_request(errors))
    }
}

// Valid registration
assert!(validate_user_registration(
    Some("user@example.com"),
    Some("johndoe"),
    Some(25)
).is_ok());

// Invalid registration - missing fields
let result = validate_user_registration(None, None, None);
assert!(result.is_err());
```

### Custom Validator Pattern

```rust
use forgedb_validation::{ValidationError, ValidationResult};

fn validate_password(password: &str) -> ValidationResult<()> {
    if password.len() < 8 {
        return Err(
            ValidationError::new("Password must be at least 8 characters")
                .with_suggestion("Use a longer password with mixed characters")
        );
    }
    
    if !password.chars().any(|c| c.is_uppercase()) {
        return Err(
            ValidationError::new("Password must contain at least one uppercase letter")
                .with_suggestion("Add uppercase letters (A-Z) to your password")
        );
    }
    
    if !password.chars().any(|c| c.is_lowercase()) {
        return Err(
            ValidationError::new("Password must contain at least one lowercase letter")
                .with_suggestion("Add lowercase letters (a-z) to your password")
        );
    }
    
    if !password.chars().any(|c| c.is_numeric()) {
        return Err(
            ValidationError::new("Password must contain at least one digit")
                .with_suggestion("Add numbers (0-9) to your password")
        );
    }
    
    Ok(())
}

// Test custom validator
match validate_password("weak") {
    Ok(_) => println!("Password is strong"),
    Err(err) => {
        println!("{}", err);
        // Output: "Error: Password must be at least 8 characters"
        //         "  Suggestion: Use a longer password with mixed characters"
    }
}
```

## API Reference

### Core Types

#### `Position`

Represents a position in source code for error reporting.

```rust
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self;
}
```

#### `ValidationError`

A validation error with optional position and suggestion.

```rust
pub struct ValidationError {
    pub message: String,
    pub position: Option<Position>,
    pub suggestion: Option<String>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self;
    pub fn with_position(self, position: Position) -> Self;
    pub fn with_suggestion(self, suggestion: impl Into<String>) -> Self;
}

// Implements Display for formatted error output
impl std::fmt::Display for ValidationError;
impl std::error::Error for ValidationError;
```

**Example:**
```rust
let error = ValidationError::new("Invalid field name")
    .with_position(Position::new(10, 5))
    .with_suggestion("Use snake_case naming");

println!("{}", error);
// Output: "Error at line 10, column 5: Invalid field name"
//         "  Suggestion: Use snake_case naming"
```

#### `ValidationResult<T>`

Type alias for validation results.

```rust
pub type ValidationResult<T> = Result<T, ValidationError>;
```

### Schema Validation Functions

#### Naming Convention Checks

```rust
// Check if string follows snake_case convention
pub fn is_snake_case(s: &str) -> bool;

// Check if string follows PascalCase convention
pub fn is_pascal_case(s: &str) -> bool;

// Convert string to snake_case
pub fn to_snake_case(s: &str) -> String;

// Convert string to PascalCase
pub fn to_pascal_case(s: &str) -> String;
```

#### Validation Functions

```rust
// Validate field name follows snake_case
pub fn validate_field_name(name: &str, position: Option<Position>) -> ValidationResult<()>;

// Validate model name follows PascalCase
pub fn validate_model_name(name: &str, position: Option<Position>) -> ValidationResult<()>;

// Check for duplicate field names
pub fn check_duplicate_fields(fields: &[(String, Option<Position>)]) -> ValidationResult<()>;

// Check for duplicate model names
pub fn check_duplicate_models(models: &[(String, Option<Position>)]) -> ValidationResult<()>;
```

### HTTP Validation Types

#### `HttpValidationError`

HTTP-specific validation error with status code.

```rust
pub struct HttpValidationError {
    pub status_code: u16,
    pub errors: Vec<ValidationError>,
}

impl HttpValidationError {
    // Create a bad request error (400)
    pub fn bad_request(errors: Vec<ValidationError>) -> Self;
    
    // Create a not found error (404)
    pub fn not_found(message: impl Into<String>) -> Self;
    
    // Create a conflict error (409)
    pub fn conflict(message: impl Into<String>) -> Self;
    
    // Create an unprocessable entity error (422)
    pub fn unprocessable_entity(errors: Vec<ValidationError>) -> Self;
    
    // Create an internal server error (500)
    pub fn internal_error(message: impl Into<String>) -> Self;
    
    // Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool;
    
    // Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool;
    
    // Get the primary error message
    pub fn message(&self) -> String;
}

impl std::fmt::Display for HttpValidationError;
impl std::error::Error for HttpValidationError;
```

#### `HttpValidator`

HTTP request validation utilities.

```rust
pub struct HttpValidator;

impl HttpValidator {
    // Validate required fields are present
    pub fn validate_required_fields(
        fields: &[(&str, Option<&str>)],
    ) -> Result<(), Vec<ValidationError>>;
    
    // Validate email format (basic check)
    pub fn validate_email(email: &str) -> Result<(), ValidationError>;
    
    // Validate string length
    pub fn validate_length(
        field_name: &str,
        value: &str,
        min: usize,
        max: usize,
    ) -> Result<(), ValidationError>;
    
    // Validate numeric range
    pub fn validate_range<T: PartialOrd + std::fmt::Display>(
        field_name: &str,
        value: T,
        min: T,
        max: T,
    ) -> Result<(), ValidationError>;
}
```

#### `StatusCodeMapper`

HTTP status code mapping utilities.

```rust
pub struct StatusCodeMapper;

impl StatusCodeMapper {
    // Get status code for validation error type
    pub fn for_validation_error(error_type: &str) -> u16;
    
    // Get status code name
    pub fn status_name(code: u16) -> &'static str;
    
    // Check if status code indicates success (2xx)
    pub fn is_success(code: u16) -> bool;
    
    // Check if status code indicates client error (4xx)
    pub fn is_client_error(code: u16) -> bool;
    
    // Check if status code indicates server error (5xx)
    pub fn is_server_error(code: u16) -> bool;
}
```

## Error Reporting

### Position Information

Validation errors can track precise source code locations:

```rust
use forgedb_validation::{ValidationError, Position};

let error = ValidationError::new("Field name must be in snake_case")
    .with_position(Position::new(42, 8));

println!("{}", error);
// Output: "Error at line 42, column 8: Field name must be in snake_case"
```

### Suggestions

Errors can include helpful suggestions for fixing issues:

```rust
use forgedb_validation::ValidationError;

let error = ValidationError::new("Model name 'user_model' must be in PascalCase")
    .with_suggestion("Consider using 'UserModel'");

println!("{}", error);
// Output: "Error: Model name 'user_model' must be in PascalCase"
//         "  Suggestion: Consider using 'UserModel'"
```

### Error Formatting

The `Display` implementation provides clear, formatted error messages:

```rust
use forgedb_validation::{ValidationError, Position};

// Error with position and suggestion
let error = ValidationError::new("Invalid field name 'UserName'")
    .with_position(Position::new(15, 3))
    .with_suggestion("Use 'user_name' instead");

println!("{}", error);
// Output:
// Error at line 15, column 3: Invalid field name 'UserName'
//   Suggestion: Use 'user_name' instead

// Error without position
let error = ValidationError::new("Email is required");
println!("{}", error);
// Output: Error: Email is required
```

### Collecting Multiple Errors

Validate multiple fields and collect all errors:

```rust
use forgedb_validation::{ValidationError, HttpValidator};

let mut errors = Vec::new();

// Validate multiple fields
if let Err(err) = HttpValidator::validate_email("invalid") {
    errors.push(err);
}

if let Err(err) = HttpValidator::validate_length("name", "X", 2, 50) {
    errors.push(err);
}

if let Err(err) = HttpValidator::validate_range("age", -1, 0, 150) {
    errors.push(err);
}

// Report all errors at once
if !errors.is_empty() {
    for error in errors {
        eprintln!("{}", error);
    }
}
```

## Testing

### Running Tests

Run all validation tests:

```bash
cargo test --package forgedb-validation
```

Run specific test module:

```bash
cargo test --package forgedb-validation --test lib_tests
cargo test --package forgedb-validation --test http_tests
cargo test --package forgedb-validation --test status_tests
```

Run with output:

```bash
cargo test --package forgedb-validation -- --nocapture
```

### Test Coverage

The test suite includes comprehensive coverage:

**Schema Validation Tests (`lib_tests.rs`):**
- ✅ snake_case and PascalCase detection
- ✅ Case conversion (to_snake_case, to_pascal_case)
- ✅ Field name validation with suggestions
- ✅ Model name validation with suggestions
- ✅ Duplicate field detection
- ✅ Duplicate model detection
- ✅ Position tracking in errors
- ✅ Error formatting with/without suggestions
- ✅ Edge cases (single character, empty strings, special characters)

**HTTP Validation Tests (`http_tests.rs`):**
- ✅ HttpValidationError creation (all status codes)
- ✅ Client error vs server error detection
- ✅ Required field validation
- ✅ Email format validation
- ✅ String length validation
- ✅ Numeric range validation

**Status Code Mapping Tests (`status_tests.rs`):**
- ✅ Error type to status code mapping
- ✅ Status code name resolution
- ✅ Success/client error/server error detection

### Example Test

```rust
use forgedb_validation::{validate_field_name, Position};

#[test]
fn test_field_validation_with_suggestion() {
    let result = validate_field_name("UserName", Some(Position::new(10, 5)));
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    
    // Check error message
    assert!(err.message.contains("snake_case"));
    
    // Check position
    assert_eq!(err.position, Some(Position::new(10, 5)));
    
    // Check suggestion
    assert!(err.suggestion.is_some());
    assert!(err.suggestion.unwrap().contains("user_name"));
}
```

## Design Decisions

### Why Position Tracking?

Position information enables:
- Precise error location in source files
- Better IDE integration (jump to error)
- Clear error messages for users
- Easier debugging during schema development

### Why Separate Schema and HTTP Validation?

The crate separates concerns:
- **Schema validation** (`lib.rs`) - Design-time validation of schema definitions
- **HTTP validation** (`http.rs`) - Runtime validation of HTTP requests
- **Status mapping** (`status.rs`) - Shared HTTP status code utilities

This separation allows:
- Using schema validation in parsers/compilers
- Using HTTP validation in web servers
- Minimal dependencies for each use case

### Why Include Suggestions?

Automatic suggestions:
- Reduce cognitive load on users
- Speed up development cycle
- Teach naming conventions
- Prevent common mistakes

### Zero Dependencies

The crate has no external dependencies, making it:
- Fast to compile
- Easy to audit
- Suitable for embedded use
- Minimal supply chain risk

## Related Crates

- **[forgedb-parser](../parser)**: Uses validation for schema parsing errors
- **[forgedb-types](../types)**: Defines types that are validated

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
