use crate::ui;
use forgedb_parser::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    pub errors: usize,
    pub warnings: usize,
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }
}

pub fn report(diagnostics: &[ValidationError]) -> Report {
    let mut counts = Report::default();

    for d in diagnostics.iter().filter(|d| d.is_warning()) {
        ui::warning_diagnostic(&d.to_string());
        counts.warnings += 1;
    }
    for d in diagnostics.iter().filter(|d| !d.is_warning()) {
        ui::error(&d.to_string());
        counts.errors += 1;
    }

    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::{Position, Severity};

    fn err(msg: &str) -> ValidationError {
        ValidationError::new(msg)
    }

    fn warn(msg: &str) -> ValidationError {
        ValidationError::warning(msg)
    }

    #[test]
    fn warnings_alone_never_report_an_error() {
        let counts = report(&[warn("char(n) is deprecated"), warn("@length(N) changed")]);
        assert_eq!(counts.errors, 0, "warnings must never count as errors");
        assert_eq!(counts.warnings, 2);
        assert!(!counts.has_errors());
    }

    #[test]
    fn errors_are_counted_as_errors() {
        let counts = report(&[err("duplicate model 'User'")]);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.warnings, 0);
        assert!(counts.has_errors());
    }

    #[test]
    fn mixed_lists_partition_by_severity() {
        let counts = report(&[warn("deprecated"), err("dangling relation"), warn("also")]);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.warnings, 2);
    }

    #[test]
    fn empty_reports_nothing() {
        let counts = report(&[]);
        assert_eq!(counts, Report::default());
        assert!(!counts.has_errors());
    }

    #[test]
    fn default_severity_is_error() {
        assert_eq!(Severity::default(), Severity::Error);
        assert!(!ValidationError::new("x").is_warning());
        assert!(ValidationError::warning("x").is_warning());
    }

    #[test]
    fn display_renders_the_severity_label() {
        let w = ValidationError::warning("char(n) is deprecated")
            .with_position(Position::new(3, 7))
            .with_suggestion("use bytes(n)");
        let text = w.to_string();
        assert!(text.starts_with("Warning at line 3, column 7:"), "{text}");
        assert!(text.contains("Suggestion: use bytes(n)"));

        let e = ValidationError::new("boom");
        assert_eq!(e.to_string(), "Error: boom");
    }

    #[test]
    fn with_severity_overrides_the_constructor() {
        assert!(ValidationError::new("x")
            .with_severity(Severity::Warning)
            .is_warning());
        assert!(!ValidationError::warning("x")
            .with_severity(Severity::Error)
            .is_warning());
    }
}
