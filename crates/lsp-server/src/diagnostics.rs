// Map compiler diagnostics onto LSP diagnostics.
//
// The LSP does NOT reimplement validation. `forgedb_parser::Parser::parse_recover`
// returns every diagnostic the compiler would — recovered syntax errors merged with
// the semantic errors from `forgedb_parser::validate_schema` — each carrying a 1-based
// source `Position`. This module is the thin adapter that turns those into 0-based LSP
// `Diagnostic`s, so editor squiggles match `forgedb validate` exactly (WS3 parity).

use forgedb_validation::ValidationError;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

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
                severity: Some(DiagnosticSeverity::ERROR),
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
