// ForgeDB Language Server — reusable library surface.
//
// Editor features (diagnostics, completion, hover, goto-definition, rename) are
// driven by the REAL ForgeDB compiler. Buffers are parsed with
// `forgedb_parser::Parser::parse_recover` — a resilient partial parse that yields
// a best-effort AST plus every diagnostic the compiler would emit — so what the
// editor shows matches `forgedb validate` exactly. There is no private grammar here.
//
// The whole server (the tower-lsp event loop and its `Backend`) lives in this
// library so it can be embedded by the `forgedb-lsp` binary that ships alongside
// the `forgedb` CLI (epic #173: one dist app, `forgedb-lsp` gated behind the
// non-default `lsp` feature of the root crate). The building blocks are also
// exercised directly: the CLI↔LSP diagnostic-parity fixture (#173,
// `tests/lsp_cli_parity.rs` in the root crate) imports `to_lsp_diagnostics` and
// asserts it stays in lockstep with `forgedb validate` over `examples/*`.

pub mod completion;
pub mod diagnostics;
pub mod hover;

pub use diagnostics::to_lsp_diagnostics;

use forgedb_parser::{ParsedSchema, Parser, Schema};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::{completion::get_completions, hover::get_hover_info};

/// Run the ForgeDB language server over stdio until the client disconnects.
///
/// Owns its own multi-threaded Tokio runtime so the caller (the `forgedb-lsp`
/// binary) stays a plain synchronous `fn main` — the async LSP stack never
/// leaks into the root `forgedb` crate's default build. Blocks until the LSP
/// session ends.
pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the Tokio runtime for the ForgeDB language server");
    runtime.block_on(serve());
}

async fn serve() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[derive(Debug, Clone)]
struct Document {
    content: String,
    parsed: ParsedSchema,
}

struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<String, Document>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Backend {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn update_document(&self, uri: Url, content: String, version: i32) {
        let (parsed, mut lsp_diagnostics) = match Parser::new(&content) {
            // Resilient parse: partial AST + all compiler diagnostics.
            Ok(mut parser) => {
                let parsed = parser.parse_recover();
                let diags = diagnostics::to_lsp_diagnostics(&parsed.diagnostics, &content);
                (parsed, diags)
            }
            // Lexer errors are fatal (no token stream to recover from). Surface the
            // message as a single diagnostic and keep an empty schema.
            Err(message) => (empty_parsed(), vec![lexer_error_diagnostic(&message)]),
        };

        // Sort by position so the editor's problem list is stable.
        lsp_diagnostics.sort_by_key(|d| (d.range.start.line, d.range.start.character));

        let doc = Document { content, parsed };
        self.documents.write().await.insert(uri.to_string(), doc);

        self.client
            .publish_diagnostics(uri, lsp_diagnostics, Some(version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        "@".to_string(),
                        "+".to_string(),
                        "&".to_string(),
                        "^".to_string(),
                        "*".to_string(),
                        "?".to_string(),
                        "[".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ForgeDB LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;
        self.update_document(uri, content, version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // Under FULL sync the spec sends exactly one element, but take `.last()`
        // so that if multiple incremental-style changes are ever batched we apply
        // the most-recent one rather than an earlier intermediate state.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(uri, change.text, version).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Saved: {}", params.text_document.uri),
            )
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string()) {
            let completions = get_completions(&doc.content, position, &doc.parsed.schema);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string()) {
            return Ok(get_hover_info(&doc.content, position, &doc.parsed.schema));
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        // A word under the cursor can name a model, a struct, or an enum.
        if let Some(doc) = docs.get(&uri.to_string())
            && let Some(word) = get_word_at_position(&doc.content, position)
            && let Some(def_pos) = find_definition(&doc.parsed.schema, &word)
        {
            let location = Location {
                uri: uri.clone(),
                range: Range {
                    start: def_pos,
                    end: Position {
                        line: def_pos.line,
                        character: def_pos.character + word.chars().count() as u32,
                    },
                },
            };
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
        }

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string())
            && let Some(old_name) = get_word_at_position(&doc.content, position)
        {
            let edits = find_all_references(&doc.content, &old_name, &new_name);

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);

            return Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }));
        }

        Ok(None)
    }
}

/// An empty [`ParsedSchema`] for the fatal-lexer-error path (no tokens to walk).
fn empty_parsed() -> ParsedSchema {
    ParsedSchema {
        schema: Schema {
            structs: Vec::new(),
            enums: Vec::new(),
            models: Vec::new(),
        },
        diagnostics: Vec::new(),
    }
}

/// Convert a compiler [`forgedb_validation::Position`] (1-based line/column) into an
/// LSP [`Position`] (0-based line/character).
fn to_lsp_position(pos: forgedb_validation::Position) -> Position {
    Position {
        line: pos.line.saturating_sub(1) as u32,
        character: pos.column.saturating_sub(1) as u32,
    }
}

/// Resolve a name to the position of its model / struct / enum definition.
fn find_definition(schema: &Schema, name: &str) -> Option<Position> {
    let pos = schema
        .models
        .iter()
        .find(|m| m.name == name)
        .and_then(|m| m.position)
        .or_else(|| {
            schema
                .structs
                .iter()
                .find(|s| s.name == name)
                .and_then(|s| s.position)
        })
        .or_else(|| {
            schema
                .enums
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| e.position)
        })?;
    Some(to_lsp_position(pos))
}

/// Turn a fatal lexer error string (which embeds "line N, column M") into a
/// diagnostic anchored at that position, falling back to the document start.
fn lexer_error_diagnostic(message: &str) -> Diagnostic {
    let start = parse_line_column(message)
        .map(|(l, c)| Position {
            line: (l.saturating_sub(1)) as u32,
            character: (c.saturating_sub(1)) as u32,
        })
        .unwrap_or(Position { line: 0, character: 0 });

    Diagnostic {
        range: Range {
            start,
            end: Position {
                line: start.line,
                character: start.character + 1,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("forgedb".to_string()),
        message: message.to_string(),
        ..Default::default()
    }
}

/// Extract the first `line N` / `column M` pair from a lexer error message.
fn parse_line_column(message: &str) -> Option<(usize, usize)> {
    let after = |marker: &str| -> Option<usize> {
        let idx = message.find(marker)? + marker.len();
        let digits: String = message[idx..]
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    };
    Some((after("line ")?, after("column ")?))
}

fn get_word_at_position(content: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let char_pos = position.character as usize;

    let chars: Vec<char> = line.chars().collect();
    if char_pos > chars.len() {
        return None;
    }

    let mut start = char_pos;
    let mut end = char_pos;

    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

fn find_all_references(content: &str, old_name: &str, new_name: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        // `search_from` is a *byte* offset into `line`. `str::find` returns byte
        // offsets, so all arithmetic here is byte-based. We convert to char counts
        // only when building LSP `Position` values (UTF-16 units; for ASCII schema
        // names char count == byte count == UTF-16 unit count).
        let mut search_from: usize = 0;

        while search_from <= line.len() {
            let Some(pos) = line[search_from..].find(old_name) else {
                break;
            };
            let byte_start = search_from + pos;
            let byte_end = byte_start + old_name.len();

            let is_word_start = line[..byte_start]
                .chars()
                .last()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            let is_word_end = line[byte_end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');

            if is_word_start && is_word_end {
                let char_start = line[..byte_start].chars().count() as u32;
                let char_end = char_start + old_name.chars().count() as u32;

                edits.push(TextEdit {
                    range: Range {
                        start: Position {
                            line: line_idx as u32,
                            character: char_start,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: char_end,
                        },
                    },
                    new_text: new_name.to_string(),
                });
            }

            search_from = byte_end;
        }
    }

    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Schema {
        Parser::new(src).unwrap().parse_recover().schema
    }

    /// goto_definition finds a model definition and returns a 0-based position.
    #[test]
    fn find_model_definition_returns_zero_based_position() {
        let schema = parse("User {\n  id: +uuid\n}\n");
        let pos = find_definition(&schema, "User").expect("should find User");
        assert_eq!(pos.line, 0);
    }

    /// goto_definition returns None for an unknown name.
    #[test]
    fn find_definition_unknown_returns_none() {
        let schema = parse("User {\n  id: +uuid\n}\n");
        assert!(find_definition(&schema, "Post").is_none());
    }

    /// goto_definition resolves struct definitions.
    #[test]
    fn find_struct_definition_returns_position() {
        let schema = parse("struct Address {\n  street: string\n}\n");
        let pos = find_definition(&schema, "Address").expect("should find Address");
        assert_eq!(pos.line, 0);
    }

    /// goto_definition resolves enum definitions (new in the compiler re-point).
    #[test]
    fn find_enum_definition_returns_position() {
        let schema = parse("enum Status {\n  Active\n  Inactive\n}\n");
        let pos = find_definition(&schema, "Status").expect("should find Status");
        assert_eq!(pos.line, 0);
    }

    /// 1-based compiler positions convert to 0-based LSP positions.
    #[test]
    fn position_conversion_is_zero_based() {
        let p = to_lsp_position(forgedb_validation::Position { line: 3, column: 5 });
        assert_eq!(p.line, 2);
        assert_eq!(p.character, 4);
    }

    /// Lexer error messages yield a diagnostic anchored at the reported position.
    #[test]
    fn lexer_error_is_positioned() {
        let d = lexer_error_diagnostic("Unexpected character '#' at line 2, column 4");
        assert_eq!(d.range.start.line, 1);
        assert_eq!(d.range.start.character, 3);
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
    }

    /// rename collects word-boundary references across the buffer.
    #[test]
    fn rename_finds_word_boundary_references() {
        let content = "User {\n  id: +uuid\n}\n\nPost {\n  author: *User\n}\n";
        let edits = find_all_references(content, "User", "Account");
        assert_eq!(edits.len(), 2, "should rename both User occurrences");
    }
}
