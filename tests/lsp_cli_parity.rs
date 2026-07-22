//! CLI ↔ LSP diagnostic parity — the anti-drift guarantee for epic #173 WS3 (#176).
//!
//! `forgedb validate` (the CLI) and the ForgeDB language server must surface the
//! *same* diagnostic set for any schema. Both derive it from a single shared seam,
//! `forgedb_parser::Parser::parse_recover` — a resilient parse returning every
//! syntax and semantic diagnostic at once:
//!
//! * the CLI reaches it through `forgedb::commands::validate::parse_and_validate`,
//! * the LSP calls `parse_recover` in `update_document`, then maps the result with
//!   `forgedb_lsp_server::to_lsp_diagnostics`.
//!
//! This fixture imports **both real code paths** and asserts they agree, so the two
//! surfaces cannot drift onto private notions of validity. It runs over the entire
//! `examples/*` corpus (which must stay clean under both) plus a battery of
//! intentionally-broken schemas that exercise non-empty parity — the case that
//! actually catches drift.

use forgedb::commands::validate::parse_and_validate;
use forgedb_lsp_server::to_lsp_diagnostics;

/// `(label, source)` for every `examples/*/schema.forge`.
fn example_schemas() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("examples directory should exist") {
        let schema = entry.expect("dir entry").path().join("schema.forge");
        if schema.is_file() {
            let content = std::fs::read_to_string(&schema).expect("read schema.forge");
            out.push((schema.display().to_string(), content));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!out.is_empty(), "expected at least one example schema");
    out
}

/// The core parity assertion: the CLI's diagnostic vector and the LSP's mapped
/// diagnostics are the same set — same count, same order, each CLI message carried
/// verbatim into the LSP message, and 1-based positions converted to 0-based ranges.
fn assert_parity(label: &str, content: &str) {
    let parsed =
        parse_and_validate(content).unwrap_or_else(|e| panic!("{label}: fatal lexer error: {e}"));
    let cli = &parsed.diagnostics;
    let lsp = to_lsp_diagnostics(cli, content);

    assert_eq!(
        cli.len(),
        lsp.len(),
        "{label}: CLI reported {} diagnostic(s) but the LSP mapper produced {} — \
         the mapper dropped, added, or reordered diagnostics",
        cli.len(),
        lsp.len(),
    );

    for (c, l) in cli.iter().zip(lsp.iter()) {
        // The LSP may append "\nSuggestion: …"; the CLI message must be a prefix.
        assert!(
            l.message.starts_with(&c.message),
            "{label}: LSP message {:?} does not carry the CLI message {:?}",
            l.message,
            c.message,
        );
        // Position parity: 1-based compiler position -> 0-based LSP range start.
        if let Some(pos) = c.position {
            assert_eq!(
                l.range.start.line,
                pos.line.saturating_sub(1) as u32,
                "{label}: line mismatch for {:?}",
                c.message,
            );
            assert_eq!(
                l.range.start.character,
                pos.column.saturating_sub(1) as u32,
                "{label}: column mismatch for {:?}",
                c.message,
            );
        }
    }
}

/// Every shipped example validates clean under BOTH surfaces (the corpus is the
/// shared fixture #176 calls out): the CLI reports zero diagnostics and the LSP
/// maps that to zero squiggles.
#[test]
fn examples_are_clean_for_both_cli_and_lsp() {
    for (label, content) in example_schemas() {
        let parsed = parse_and_validate(&content)
            .unwrap_or_else(|e| panic!("{label}: fatal lexer error: {e}"));
        assert!(
            parsed.diagnostics.is_empty(),
            "{label}: expected a clean example, got {:?}",
            parsed
                .diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>(),
        );
        assert!(
            to_lsp_diagnostics(&parsed.diagnostics, &content).is_empty(),
            "{label}: LSP produced diagnostics for a clean example",
        );
        assert_parity(&label, &content);
    }
}

/// Non-empty parity — the case that actually guards against drift. Each fixture
/// trips a distinct compiler rule; the CLI and the LSP must report the identical
/// diagnostic set for it.
#[test]
fn invalid_schemas_produce_matching_diagnostic_sets() {
    let fixtures = [
        (
            "duplicate-model",
            "User {\n  id: +uuid\n}\n\nUser {\n  id: +uuid\n}\n",
        ),
        ("bad-model-name", "user {\n  id: +uuid\n}\n"),
        ("bad-field-name", "User {\n  BadName: string\n}\n"),
        (
            "dangling-relation",
            "User {\n  id: +uuid\n  posts: [Ghost]\n}\n",
        ),
    ];

    for (label, src) in fixtures {
        let parsed =
            parse_and_validate(src).unwrap_or_else(|e| panic!("{label}: fatal lexer error: {e}"));
        assert!(
            !parsed.diagnostics.is_empty(),
            "{label}: fixture was expected to produce diagnostics but validated clean",
        );
        assert_parity(label, src);
    }
}
