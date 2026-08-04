//! Rendering and partitioning of compiler diagnostics for the CLI (#237).
//!
//! Before the severity axis existed, every diagnostic path was error-or-nothing,
//! so both CLI surfaces could treat "the diagnostic list is non-empty" as "this
//! failed". That shortcut is exactly what a warning breaks, and it is why this
//! module exists rather than each command partitioning inline: a deprecation that
//! fails a build is a removal, not a deprecation, and shipping that on a *minor*
//! version would break every downstream user's CI.
//!
//! The rule this module enforces, in one place, for every caller:
//!
//! > **Only [`Report::errors`] may influence an exit code.**
//!
//! [`report`] prints everything and counts by severity; the caller decides what to
//! do with the error count alone. A caller that consults [`Report::warnings`] when
//! choosing its exit status is reintroducing the bug.

use crate::ui;
// Via the parser's re-export rather than a direct `forgedb-validation` dependency:
// the CLI already depends on `forgedb-parser`, and every diagnostic it renders
// arrives through `ParsedSchema` or `Parser::warnings` anyway.
use forgedb_parser::ValidationError;

/// Counts from a rendered diagnostic list, partitioned by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    /// Fatal diagnostics. **The only field that may drive an exit code.**
    pub errors: usize,
    /// Advisory diagnostics. Informational only — never fatal, in any mode,
    /// including `--strict` (strictness escalates *lints*, not deprecations; a
    /// deprecation the user cannot yet act on must not break their build).
    pub warnings: usize,
}

impl Report {
    /// Whether anything fatal was reported.
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }
}

/// Print each diagnostic on the stream matching its severity and return the
/// counts. Errors and warnings both go to stderr (see [`ui::warning_diagnostic`]).
///
/// Warnings are printed first so that when both are present the fatal ones sit
/// closest to the failure message that follows.
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

    /// The load-bearing invariant: a list of nothing but warnings reports **zero**
    /// errors, so no caller can turn it into a non-zero exit. If this ever fails,
    /// every deprecation built on this channel becomes a breaking change.
    #[test]
    fn warnings_alone_never_report_an_error() {
        let counts = report(&[warn("char(n) is deprecated"), warn("@length(N) changed")]);
        assert_eq!(counts.errors, 0, "warnings must never count as errors");
        assert_eq!(counts.warnings, 2);
        assert!(!counts.has_errors());
    }

    /// Errors still count as errors — this issue must not soften anything fatal.
    #[test]
    fn errors_are_counted_as_errors() {
        let counts = report(&[err("duplicate model 'User'")]);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.warnings, 0);
        assert!(counts.has_errors());
    }

    /// A mixed list partitions rather than collapsing to one bucket.
    #[test]
    fn mixed_lists_partition_by_severity() {
        let counts = report(&[warn("deprecated"), err("dangling relation"), warn("also")]);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.warnings, 2);
    }

    /// An empty list reports nothing — the pre-#237 fast path is unchanged.
    #[test]
    fn empty_reports_nothing() {
        let counts = report(&[]);
        assert_eq!(counts, Report::default());
        assert!(!counts.has_errors());
    }

    /// `Severity::Error` is the default, which is what makes the field additive:
    /// every construction that predates #237 keeps its exact meaning.
    #[test]
    fn default_severity_is_error() {
        assert_eq!(Severity::default(), Severity::Error);
        assert!(!ValidationError::new("x").is_warning());
        assert!(ValidationError::warning("x").is_warning());
    }

    /// Display renders the severity word, so a warning never prints as "Error".
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

    /// `with_severity` can promote or demote after construction.
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
