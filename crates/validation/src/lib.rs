pub mod http;
pub mod status;

use std::collections::HashSet;

pub use http::{HttpValidationError, HttpValidator};
pub use status::StatusCodeMapper;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub message: String,
    pub position: Option<Position>,
    pub suggestion: Option<String>,
    pub severity: Severity,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        ValidationError {
            message: message.into(),
            position: None,
            suggestion: None,
            severity: Severity::Error,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        ValidationError {
            message: message.into(),
            position: None,
            suggestion: None,
            severity: Severity::Warning,
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

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn is_warning(&self) -> bool {
        self.severity == Severity::Warning
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = self.severity.label();
        if let Some(pos) = self.position {
            write!(
                f,
                "{} at line {}, column {}: {}",
                label, pos.line, pos.column, self.message
            )?;
        } else {
            write!(f, "{}: {}", label, self.message)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

pub fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    if !s.chars().next().unwrap().is_lowercase() && !s.starts_with('_') {
        return false;
    }

    s.chars()
        .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    if !s.chars().next().unwrap().is_uppercase() {
        return false;
    }

    s.chars().all(|c| c.is_alphanumeric()) && !s.contains('_')
}

pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];

        if c.is_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

                if prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next_is_lower)
                {
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
