// Sprint 22: Language Server Protocol for ForgeDB
//
// Provides rich IDE features:
// - Real-time diagnostics
// - Code completion
// - Hover information
// - Go to definition
// - Rename refactoring

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

mod parser;
mod diagnostics;
mod completion;
mod hover;

use parser::{parse_schema, Schema, Model, Field};
use diagnostics::validate_schema;
use completion::get_completions;
use hover::get_hover_info;

#[derive(Debug)]
struct Document {
    uri: Url,
    content: String,
    schema: Option<Schema>,
    version: i32,
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

    async fn get_document(&self, uri: &Url) -> Option<Document> {
        let docs = self.documents.read().await;
        docs.get(&uri.to_string()).cloned()
    }

    async fn update_document(&self, uri: Url, content: String, version: i32) {
        let schema = parse_schema(&content);
        let doc = Document {
            uri: uri.clone(),
            content: content.clone(),
            schema: schema.clone(),
            version,
        };

        let mut docs = self.documents.write().await;
        docs.insert(uri.to_string(), doc);

        // Publish diagnostics
        if let Some(schema) = schema {
            let diagnostics = validate_schema(&schema, &content);
            self.client.publish_diagnostics(uri, diagnostics, Some(version)).await;
        }
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

        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_document(uri, change.text, version).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, format!("Saved: {}", params.text_document.uri))
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string()) {
            let completions = get_completions(&doc.content, position, &doc.schema);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string()) {
            return Ok(get_hover_info(&doc.content, position, &doc.schema));
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
        if let Some(doc) = docs.get(&uri.to_string()) {
            if let Some(schema) = &doc.schema {
                // Find word at position
                if let Some(word) = get_word_at_position(&doc.content, position) {
                    // Search for model definition
                    if let Some(model_pos) = find_model_definition(schema, &word) {
                        let location = Location {
                            uri: uri.clone(),
                            range: Range {
                                start: model_pos,
                                end: Position {
                                    line: model_pos.line,
                                    character: model_pos.character + word.len() as u32,
                                },
                            },
                        };
                        return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri.to_string()) {
            if let Some(schema) = &doc.schema {
                if let Some(old_name) = get_word_at_position(&doc.content, position) {
                    // Find all references
                    let edits = find_all_references(schema, &doc.content, &old_name, &new_name);

                    let mut changes = HashMap::new();
                    changes.insert(uri.clone(), edits);

                    return Ok(Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }));
                }
            }
        }

        Ok(None)
    }
}

fn get_word_at_position(content: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let char_pos = position.character as usize;

    if char_pos > line.len() {
        return None;
    }

    // Find word boundaries
    let mut start = char_pos;
    let mut end = char_pos;

    let chars: Vec<char> = line.chars().collect();

    // Go backwards to find start
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    // Go forwards to find end
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

fn find_model_definition(schema: &Schema, model_name: &str) -> Option<Position> {
    schema.models.iter()
        .find(|m| m.name == model_name)
        .map(|m| m.position)
}

fn find_all_references(
    schema: &Schema,
    content: &str,
    old_name: &str,
    new_name: &str,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let mut col = 0;
        while let Some(pos) = line[col..].find(old_name) {
            let actual_col = col + pos;

            // Check if it's a whole word match
            let is_word_start = actual_col == 0 ||
                !line.chars().nth(actual_col - 1).unwrap().is_alphanumeric();
            let is_word_end = actual_col + old_name.len() >= line.len() ||
                !line.chars().nth(actual_col + old_name.len()).unwrap().is_alphanumeric();

            if is_word_start && is_word_end {
                edits.push(TextEdit {
                    range: Range {
                        start: Position {
                            line: line_idx as u32,
                            character: actual_col as u32,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: (actual_col + old_name.len()) as u32,
                        },
                    },
                    new_text: new_name.to_string(),
                });
            }

            col = actual_col + 1;
        }
    }

    edits
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
