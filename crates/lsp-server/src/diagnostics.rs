// Map compiler diagnostics onto LSP diagnostics.
//
// The LSP does NOT reimplement validation. `forgedb_parser::Parser::parse_recover`
// returns every diagnostic the compiler would — recovered syntax errors merged with
// the semantic errors from `forgedb_parser::validate_schema` — each carrying a 1-based
// source `Position`. This module is the thin adapter that turns those into 0-based LSP
// `Diagnostic`s, so editor squiggles match `forgedb validate` exactly (#173 parity).

use forgedb_validation::{Severity, ValidationError};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Map a compiler [`Severity`] onto the LSP one (#237).
///
/// Before the severity axis existed this was hardcoded to `ERROR`, which made a
/// deprecation indistinguishable from a broken schema in the editor.
pub(crate) fn to_lsp_severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    }
}

/// Convert compiler diagnostics (1-based line/column) into LSP diagnostics (0-based).
///
/// `content` is consulted only to size each squiggle to the identifier under the
/// reported position; the diagnostics themselves come straight from the compiler.
pub fn to_lsp_diagnostics(errors: &[ValidationError], content: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = content.lines().collect();

    errors
        .iter()
        .map(|err| {
            let range = err
                .position
                .map(|pos| range_at(&lines, pos.line, pos.column))
                .unwrap_or_else(|| Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                });

            let message = match &err.suggestion {
                Some(s) => format!("{}\nSuggestion: {}", err.message, s),
                None => err.message.clone(),
            };

            Diagnostic {
                range,
                severity: Some(to_lsp_severity(err.severity)),
                source: Some("forgedb".to_string()),
                message,
                ..Default::default()
            }
        })
        .collect()
}

/// Build a 0-based LSP range covering the identifier starting at a 1-based
/// (`line`, `column`) source position. Falls back to a single-character span when
/// the position lands past end-of-line (e.g. a "missing token" diagnostic).
fn range_at(lines: &[&str], line: usize, column: usize) -> Range {
    let line0 = line.saturating_sub(1);
    let col0 = column.saturating_sub(1);

    let word_len = lines
        .get(line0)
        .map(|l| {
            l.chars()
                .skip(col0)
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count()
        })
        .filter(|n| *n > 0)
        .unwrap_or(1);

    Range {
        start: Position {
            line: line0 as u32,
            character: col0 as u32,
        },
        end: Position {
            line: line0 as u32,
            character: (col0 + word_len) as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::Parser;

    /// A duplicate-name schema produces a positioned ERROR diagnostic sourced "forgedb".
    #[test]
    fn duplicate_model_maps_to_positioned_error() {
        let src = "User {\n  id: +uuid\n}\n\nUser {\n  id: +uuid\n}\n";
        let parsed = Parser::new(src).unwrap().parse_recover();
        let diags = to_lsp_diagnostics(&parsed.diagnostics, src);
        assert!(!diags.is_empty(), "expected at least one diagnostic");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.source.as_deref(), Some("forgedb"));
    }

    /// A naming-convention violation surfaces the compiler's suggestion in the message.
    #[test]
    fn suggestion_is_appended_to_message() {
        let src = "user {\n  id: +uuid\n}\n";
        let parsed = Parser::new(src).unwrap().parse_recover();
        let diags = to_lsp_diagnostics(&parsed.diagnostics, src);
        assert!(
            diags.iter().any(|d| d.message.contains("Suggestion:")),
            "expected a suggestion in: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// A warning-severity diagnostic publishes as `WARNING`, not `ERROR` (#237).
    /// Before the severity axis existed this site was hardcoded, which would have
    /// made every deprecation look like a broken schema in the editor.
    #[test]
    fn warning_severity_maps_to_lsp_warning() {
        use forgedb_validation::{Severity, ValidationError};

        let diags = to_lsp_diagnostics(
            &[
                ValidationError::warning("char(n) is deprecated"),
                ValidationError::new("duplicate model 'User'"),
            ],
            "User {\n  id: +uuid\n}\n",
        );

        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diags[1].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            to_lsp_severity(Severity::Error),
            DiagnosticSeverity::ERROR,
            "the default severity must keep mapping to ERROR"
        );
    }

    /// 1-based compiler positions become 0-based LSP ranges spanning the identifier.
    #[test]
    fn positions_convert_to_zero_based_word_ranges() {
        let lines = ["User {", "  bad name: string", "}"];
        let r = range_at(&lines, 2, 3); // 1-based line 2, col 3 -> "bad"
        assert_eq!(r.start.line, 1);
        assert_eq!(r.start.character, 2);
        assert_eq!(r.end.character, 5, "should span the 3-char word 'bad'");
    }
}
