# ForgeDB Internal Crates

> ⚠️ **STALE — do not rely on this document (2026-07-17).** It predates the v1
> core-close and everything after it: the code-generation pipeline, crate roster,
> and migration system described below no longer match the code. Crates named here
> as live may have been pruned (`query-optimization`, `http-server`, `fulltext`,
> `crud-api`, `ffi`) and generators added since (Transform, Wasm, PyO3, NAPI) are
> absent. For the current picture use **`docs/ARCHITECTURE.md`**, **`docs/PUBLIC_CRATES.md`**,
> the root **`CLAUDE.md`** (authoritative crate/status ledger), and the code itself.
> A full rewrite is tracked separately.

**Last Updated:** October 2025 *(stale — see banner above)*  
**Audience:** ForgeDB developers, contributors

## Table of Contents

- [Overview](#overview)
- [CLI Architecture](#cli-architecture)
- [Code Generation Pipeline](#code-generation-pipeline)
- [Schema Parsing](#schema-parsing)
- [Migration System](#migration-system)
- [LSP Integration](#lsp-integration)
- [Development Workflow](#development-workflow)

---

## Overview

ForgeDB's internal crates handle **tooling and code generation**. Unlike public crates, these are not published to crates.io and are not used in production runtime. They transform schema definitions into executable code.

### Internal Crates

| Crate | Purpose | Status |
|-------|---------|--------|
| **forgedb-parser** | Schema language parser | Production |
| **forgedb-validation** | Semantic validation | Production |
| **forgedb-watcher** | File system watching | Production |
| **forgedb-migrations** | Schema migrations | Production |
| **forgedb-lsp-server** | IDE support (LSP) | Production |
| **forgedb** (binary) | Command-line interface | Production |

### Design Philosophy

Internal crates prioritize:

🚀 **Developer Experience**: Fast iteration, helpful errors  
🔧 **Flexibility**: Breaking changes acceptable  
🎯 **Code Quality**: Generated code must be production-ready  
📚 **Extensibility**: Easy to add new features  

---

## CLI Architecture

### Command Structure

```
forgedb/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── commands/         # Command implementations
│   │   ├── init.rs       # Project initialization
│   │   ├── dev.rs        # Development server
│   │   ├── build.rs      # Code generation
│   │   ├── migrate.rs    # Migrations
│   │   └── validate.rs   # Schema validation
│   ├── generator/        # Code generation
│   │   ├── rust.rs       # Rust code generation
│   │   ├── typescript.rs # TypeScript generation
│   │   ├── openapi.rs    # OpenAPI spec generation
│   │   └── templates.rs  # Code templates
│   ├── error.rs          # Error types
│   └── config.rs         # Configuration
└── Cargo.toml
```

### CLI Commands

#### `forgedb init <project_name>`

**Purpose**: Create new ForgeDB project.

**Implementation**:
```rust
pub fn init_project(name: &str, path: &Path) -> Result<()> {
    // 1. Create directory structure
    create_directory_structure(path)?;
    
    // 2. Generate schema template
    let schema = include_str!("templates/schema.forge");
    fs::write(path.join("schema.forge"), schema)?;
    
    // 3. Create Cargo.toml
    let cargo_toml = generate_cargo_toml(name)?;
    fs::write(path.join("Cargo.toml"), cargo_toml)?;
    
    // 4. Initialize git
    git_init(path)?;
    
    Ok(())
}
```

**Generated Structure**:
```
my-project/
├── schema.forge          # Schema definition
├── Cargo.toml           # Rust project config
├── src/
│   └── main.rs          # Entry point stub
├── generated/           # Generated code (gitignored)
└── .gitignore
```

#### `forgedb dev`

**Purpose**: Start development server with hot reload.

**Implementation**:
```rust
pub async fn dev_server(config: DevConfig) -> Result<()> {
    // 1. Parse and validate schema
    let schema = parse_schema("schema.forge")?;
    validate_schema(&schema)?;
    
    // 2. Generate code
    generate_code(&schema, &config.output_dir)?;
    
    // 3. Watch for changes
    let (tx, rx) = channel();
    let watcher = watch_schema_file("schema.forge", tx)?;
    
    // 4. Start server
    let server = start_http_server(&config).await?;
    
    // 5. Handle file changes
    loop {
        match rx.recv() {
            Ok(Event::SchemaChanged) => {
                println!("Schema changed, regenerating...");
                regenerate_and_reload(&schema, &server)?;
            }
            Err(_) => break,
        }
    }
    
    Ok(())
}
```

**Features**:
- File watching with debouncing
- Automatic code regeneration
- Server hot reload
- Error reporting with line numbers

#### `forgedb build`

**Purpose**: Generate production code.

**Implementation**:
```rust
pub fn build_project(config: BuildConfig) -> Result<()> {
    // 1. Parse schema
    let schema_path = config.schema_path.unwrap_or("schema.forge".into());
    let schema = parse_schema(&schema_path)?;
    
    // 2. Validate
    let errors = validate_schema(&schema);
    if !errors.is_empty() {
        return Err(ValidationError::new(errors).into());
    }
    
    // 3. Generate Rust code
    generate_rust_code(&schema, &config.output_dir)?;
    
    // 4. Generate TypeScript SDK
    if config.generate_typescript {
        generate_typescript_sdk(&schema, &config.ts_output_dir)?;
    }
    
    // 5. Generate OpenAPI spec
    if config.generate_openapi {
        generate_openapi_spec(&schema, &config.openapi_output)?;
    }
    
    // 6. Format generated code
    format_generated_code(&config.output_dir)?;
    
    println!("✓ Build complete");
    Ok(())
}
```

#### `forgedb migrate`

**Purpose**: Generate and run schema migrations.

**Implementation**:
```rust
pub fn migrate(action: MigrateAction, config: MigrateConfig) -> Result<()> {
    match action {
        MigrateAction::Generate => {
            // Compare old and new schema
            let old_schema = load_previous_schema(&config.data_dir)?;
            let new_schema = parse_schema("schema.forge")?;
            
            // Generate migration plan
            let migration = generate_migration_plan(&old_schema, &new_schema)?;
            
            // Save migration file
            save_migration(&migration, &config.migrations_dir)?;
            
            println!("✓ Migration generated");
        }
        
        MigrateAction::Run => {
            // Load pending migrations
            let migrations = load_pending_migrations(&config.migrations_dir)?;
            
            // Execute each migration
            for migration in migrations {
                execute_migration(&migration, &config.data_dir)?;
                mark_migration_complete(&migration)?;
                println!("✓ Applied: {}", migration.name);
            }
        }
        
        MigrateAction::Rollback => {
            // Rollback last migration
            let last = get_last_migration(&config.migrations_dir)?;
            rollback_migration(&last, &config.data_dir)?;
            println!("✓ Rolled back: {}", last.name);
        }
    }
    
    Ok(())
}
```

#### `forgedb validate`

**Purpose**: Validate schema without generating code.

**Implementation**:
```rust
pub fn validate_schema_file(path: &Path) -> Result<()> {
    // 1. Parse schema
    let schema = match parse_schema(path) {
        Ok(s) => s,
        Err(e) => {
            print_parse_error(&e, path)?;
            return Err(e.into());
        }
    };
    
    // 2. Validate semantics
    let errors = validate_schema(&schema);
    
    // 3. Report errors
    if !errors.is_empty() {
        for error in &errors {
            print_validation_error(error, path)?;
        }
        return Err(ValidationError::new(errors).into());
    }
    
    println!("✓ Schema is valid");
    Ok(())
}
```

---

## Code Generation Pipeline

### Overview

```
schema.forge
    ↓
┌─────────────────┐
│  Parser         │ → AST
└────────┬────────┘
         ↓
┌─────────────────┐
│  Validator      │ → Validated AST
└────────┬────────┘
         ↓
┌─────────────────┐
│  Generator      │ → Code Files
└────────┬────────┘
         ↓
┌─────────────────┐
│  Formatter      │ → Formatted Code
└─────────────────┘
```

### Generator Architecture

```rust
pub trait CodeGenerator {
    type Output;
    
    fn generate(&self, schema: &Schema) -> Result<Self::Output>;
}

// Rust code generator
pub struct RustGenerator {
    config: RustGenConfig,
}

impl CodeGenerator for RustGenerator {
    type Output = HashMap<PathBuf, String>;
    
    fn generate(&self, schema: &Schema) -> Result<Self::Output> {
        let mut files = HashMap::new();
        
        // Generate for each model
        for model in &schema.models {
            files.insert(
                PathBuf::from(format!("{}.rs", model.name.to_lowercase())),
                self.generate_model(model)?
            );
        }
        
        // Generate main module
        files.insert(
            PathBuf::from("lib.rs"),
            self.generate_lib(schema)?
        );
        
        Ok(files)
    }
}

// TypeScript generator
pub struct TypeScriptGenerator {
    config: TsGenConfig,
}

impl CodeGenerator for TypeScriptGenerator {
    type Output = HashMap<PathBuf, String>;
    
    fn generate(&self, schema: &Schema) -> Result<Self::Output> {
        let mut files = HashMap::new();
        
        // Generate types
        files.insert(
            PathBuf::from("types.ts"),
            self.generate_types(schema)?
        );
        
        // Generate API client
        files.insert(
            PathBuf::from("client.ts"),
            self.generate_client(schema)?
        );
        
        Ok(files)
    }
}
```

### Rust Code Generation

#### Model Generation

**Input Schema**:
```
User {
  id: +uuid
  email: ^&string
  name: string
  created_at: ^timestamp
}
```

**Generated Code**:
```rust
// generated/user.rs

use uuid::Uuid;
use serde::{Serialize, Deserialize};
use forgedb_storage::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInsert {
    pub email: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserUpdate {
    pub email: Option<String>,
    pub name: Option<String>,
}

pub struct UserTable {
    database: Database,
    email_index: HashMap<String, Uuid>,
}

impl UserTable {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            email_index: HashMap::new(),
        }
    }
    
    pub fn insert(&mut self, data: UserInsert) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let user = User {
            id,
            email: data.email.clone(),
            name: data.name,
            created_at: chrono::Utc::now().timestamp(),
        };
        
        // Write to storage
        self.write_to_storage(&user)?;
        
        // Update indexes
        self.email_index.insert(data.email, id);
        
        Ok(id)
    }
    
    pub fn find_by_id(&self, id: Uuid) -> Result<Option<User>> {
        self.read_from_storage(id)
    }
    
    pub fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        if let Some(id) = self.email_index.get(email) {
            self.find_by_id(*id)
        } else {
            Ok(None)
        }
    }
    
    // ... more methods
}
```

#### HTTP API Generation

**Generated Router**:
```rust
// generated/api/user_routes.rs

use axum::{Router, routing::{get, post, put, delete}};
use forgedb_http_server::*;

pub fn user_routes() -> Router {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/:id", 
            get(get_user)
            .put(update_user)
            .delete(delete_user)
        )
        .route("/users/email/:email", get(get_user_by_email))
}

async fn create_user(
    State(db): State<Arc<Database>>,
    Json(data): Json<UserInsert>,
) -> Result<Json<User>, StatusCode> {
    let id = db.users.insert(data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let user = db.users.find_by_id(id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    Ok(Json(user))
}

async fn get_user(
    State(db): State<Arc<Database>>,
    Path(id): Path<Uuid>,
) -> Result<Json<User>, StatusCode> {
    let user = db.users.find_by_id(id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    Ok(Json(user))
}

// ... more handlers
```

### TypeScript SDK Generation

**Generated Types**:
```typescript
// generated/types.ts

export interface User {
  id: string;  // UUID
  email: string;
  name: string;
  created_at: number;  // timestamp
}

export interface UserInsert {
  email: string;
  name: string;
}

export interface UserUpdate {
  email?: string;
  name?: string;
}
```

**Generated Client**:
```typescript
// generated/client.ts

export class ForgeDBClient {
  constructor(private baseUrl: string) {}
  
  async createUser(data: UserInsert): Promise<User> {
    const response = await fetch(`${this.baseUrl}/users`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    
    return response.json();
  }
  
  async getUser(id: string): Promise<User> {
    const response = await fetch(`${this.baseUrl}/users/${id}`);
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    
    return response.json();
  }
  
  // ... more methods
}
```

### OpenAPI Specification Generation

**Generated OpenAPI**:
```yaml
# generated/openapi.yaml

openapi: 3.0.0
info:
  title: ForgeDB API
  version: 1.0.0

paths:
  /users:
    post:
      summary: Create user
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UserInsert'
      responses:
        '201':
          description: Created
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
    get:
      summary: List users
      parameters:
        - name: page
          in: query
          schema:
            type: integer
        - name: limit
          in: query
          schema:
            type: integer
      responses:
        '200':
          description: Success
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: '#/components/schemas/User'

components:
  schemas:
    User:
      type: object
      properties:
        id:
          type: string
          format: uuid
        email:
          type: string
          format: email
        name:
          type: string
        created_at:
          type: integer
          format: int64
```

---

## Schema Parsing

### forgedb-parser

**Purpose**: Transform schema text into Abstract Syntax Tree (AST).

**Architecture**:
```
Input: schema.forge (text)
    ↓
Lexer: Tokenize
    ↓
Parser: Build AST
    ↓
Output: Schema (AST)
```

### Lexer

**Tokens**:
```rust
pub enum Token {
    // Keywords
    Model,
    Enum,
    
    // Identifiers
    Ident(String),
    
    // Types
    String,
    U64,
    I64,
    F64,
    Uuid,
    Timestamp,
    Bool,
    
    // Operators
    Colon,
    Plus,        // +
    Caret,       // ^
    Ampersand,   // &
    Asterisk,    // *
    
    // Delimiters
    LeftBrace,   // {
    RightBrace,  // }
    LeftBracket, // [
    RightBracket,// ]
    LeftParen,   // (
    RightParen,  // )
    
    // Directives
    At,          // @
    
    // Literals
    StringLiteral(String),
    NumberLiteral(i64),
    
    // Special
    Eof,
}
```

### Parser

**AST Structure**:
```rust
pub struct Schema {
    pub models: Vec<Model>,
    pub enums: Vec<Enum>,
}

pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub directives: Vec<Directive>,
}

pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub modifiers: FieldModifiers,
    pub directives: Vec<Directive>,
}

pub enum FieldType {
    Primitive(PrimitiveType),
    Reference(String),
    Array(Box<FieldType>),
    FixedArray(Box<FieldType>, usize),
    Component(ComponentType),
}

pub enum PrimitiveType {
    String,
    U64,
    I64,
    F64,
    Uuid,
    Timestamp,
    Bool,
}

pub struct FieldModifiers {
    pub primary_key: bool,    // +
    pub indexed: bool,        // ^
    pub unique: bool,         // &
    pub required: bool,       // *
}

pub struct Directive {
    pub name: String,
    pub args: Vec<DirectiveArg>,
}
```

**Parsing Example**:
```rust
// Input
let input = r#"
User {
  id: +uuid
  email: ^&string
  posts: [Post]
  
  @index(email, username)
}
"#;

// Parse
let schema = parse_schema(input)?;

// AST
assert_eq!(schema.models[0].name, "User");
assert_eq!(schema.models[0].fields.len(), 3);
assert_eq!(schema.models[0].fields[0].name, "id");
assert!(schema.models[0].fields[0].modifiers.primary_key);
```

### Error Reporting

**Parse Error with Context**:
```
Error: Unexpected token
  ┌─ schema.forge:5:12
  │
5 │   email: ^&strng
  │            ^^^^^^ Expected type, found 'strng'
  │
  = help: Did you mean 'string'?
```

---

## Migration System

### forgedb-migrations

**Purpose**: Handle schema evolution over time.

### Migration Types

**1. Add Field**:
```rust
pub struct AddField {
    pub model: String,
    pub field: Field,
    pub default_value: Option<Value>,
}
```

**2. Remove Field**:
```rust
pub struct RemoveField {
    pub model: String,
    pub field_name: String,
}
```

**3. Rename Field**:
```rust
pub struct RenameField {
    pub model: String,
    pub old_name: String,
    pub new_name: String,
}
```

**4. Change Type**:
```rust
pub struct ChangeFieldType {
    pub model: String,
    pub field_name: String,
    pub old_type: FieldType,
    pub new_type: FieldType,
    pub conversion: Option<ConversionFn>,
}
```

**5. Add/Remove Index**:
```rust
pub struct AddIndex {
    pub model: String,
    pub fields: Vec<String>,
}

pub struct RemoveIndex {
    pub model: String,
    pub fields: Vec<String>,
}
```

### Migration File Format

```rust
// migrations/20231015_add_user_bio.rs

use forgedb_migrations::*;

pub struct Migration20231015AddUserBio;

impl Migration for Migration20231015AddUserBio {
    fn up(&self, db: &mut Database) -> Result<()> {
        // Add 'bio' field to User model
        db.add_field("User", Field {
            name: "bio".to_string(),
            field_type: FieldType::String,
            default_value: Some(Value::String("".to_string())),
        })?;
        
        Ok(())
    }
    
    fn down(&self, db: &mut Database) -> Result<()> {
        // Remove 'bio' field from User model
        db.remove_field("User", "bio")?;
        
        Ok(())
    }
}
```

### Migration Execution

```rust
pub fn execute_migration<M: Migration>(
    migration: &M,
    database: &mut Database,
) -> Result<()> {
    // 1. Begin transaction
    let txn = database.begin_transaction()?;
    
    // 2. Execute migration
    migration.up(database)?;
    
    // 3. Update migration table
    database.record_migration(migration.id())?;
    
    // 4. Commit transaction
    txn.commit()?;
    
    Ok(())
}
```

---

## LSP Integration

### forgedb-lsp-server

**Purpose**: Provide IDE support for ForgeDB schema language.

**Features**:
- ✅ Syntax highlighting
- ✅ Auto-completion
- ✅ Go-to-definition
- ✅ Hover documentation
- ✅ Error diagnostics
- ✅ Code actions (quick fixes)

### LSP Server Architecture

```rust
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

pub struct ForgeDBLspServer {
    client: Client,
    schema_cache: Arc<RwLock<HashMap<Url, Schema>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for ForgeDBLspServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions::default()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions::default(),
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }
    
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ForgeDB LSP initialized")
            .await;
    }
    
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = &params.content_changes[0].text;
        
        // Parse schema
        match parse_schema(text) {
            Ok(schema) => {
                // Cache schema
                self.schema_cache.write().await.insert(uri.clone(), schema);
                
                // Clear diagnostics
                self.client.publish_diagnostics(uri, Vec::new(), None).await;
            }
            Err(errors) => {
                // Publish diagnostics
                let diagnostics = errors.into_iter()
                    .map(|e| Diagnostic {
                        range: e.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: e.message,
                        ..Default::default()
                    })
                    .collect();
                
                self.client.publish_diagnostics(uri, diagnostics, None).await;
            }
        }
    }
    
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        // Get schema from cache
        let schema = self.schema_cache.read().await.get(uri).cloned();
        
        // Generate completions
        let completions = generate_completions(&schema, position);
        
        Ok(Some(CompletionResponse::Array(completions)))
    }
    
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        // Get schema from cache
        let schema = self.schema_cache.read().await.get(uri).cloned();
        
        // Generate hover info
        let hover_text = generate_hover_info(&schema, position);
        
        Ok(hover_text.map(|text| Hover {
            contents: HoverContents::Scalar(MarkedString::String(text)),
            range: None,
        }))
    }
    
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        // Find definition
        let location = find_definition(&uri, position).await;
        
        Ok(location.map(GotoDefinitionResponse::Scalar))
    }
    
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
```

### VSCode Extension Integration

The LSP server is consumed by the VSCode extension (vscode-forgedb):

```typescript
// vscode-forgedb/src/extension.ts

import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

export function activate(context: vscode.ExtensionContext) {
  // Server executable
  const serverOptions: ServerOptions = {
    command: 'forgedb-lsp-server',
    args: [],
  };
  
  // Client options
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'forgedb' }],
  };
  
  // Create and start client
  const client = new LanguageClient(
    'forgedb',
    'ForgeDB Language Server',
    serverOptions,
    clientOptions
  );
  
  client.start();
  
  context.subscriptions.push(client);
}
```

---

## Development Workflow

### Local Development

**1. Clone and Build**:
```bash
git clone https://github.com/yourusername/forgedb
cd forgedb
cargo build
```

**2. Run Tests**:
```bash
cargo test --lib
cargo test --package forgedb-parser
cargo test --package forgedb-validation
```

**3. Test CLI**:
```bash
cargo run -- init test-project
cd test-project
cargo run -- dev
```

**4. Test Code Generation**:
```bash
# Create test schema
cat > schema.forge << EOF
User {
  id: +uuid
  email: ^&string
}
EOF

# Generate code
cargo run -- build

# Check generated files
ls -la generated/
```

### Testing Strategy

**Unit Tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_model() {
        let input = r#"
        User {
          id: +uuid
          email: string
        }
        "#;
        
        let schema = parse_schema(input).unwrap();
        assert_eq!(schema.models.len(), 1);
        assert_eq!(schema.models[0].name, "User");
    }
}
```

**Integration Tests**:
```rust
#[test]
fn test_full_code_generation() {
    // Parse schema
    let schema = parse_schema("test_schema.forge").unwrap();
    
    // Validate
    let errors = validate_schema(&schema);
    assert!(errors.is_empty());
    
    // Generate code
    let generated = generate_rust_code(&schema).unwrap();
    
    // Verify generated code compiles
    assert!(generated.contains("pub struct User"));
    assert!(generated.contains("pub fn insert"));
}
```

### Debugging Tips

**1. Enable Verbose Logging**:
```bash
RUST_LOG=debug cargo run -- dev
```

**2. Inspect AST**:
```rust
let schema = parse_schema("schema.forge")?;
println!("{:#?}", schema);  // Pretty-print AST
```

**3. Debug Code Generation**:
```rust
let generated = generate_rust_code(&schema)?;
println!("Generated:\n{}", generated);
```

**4. Test LSP**:
```bash
# Start LSP server manually
cargo run --bin forgedb-lsp-server

# Connect with LSP client
# Send initialize request...
```

---

## References

- [Architecture Overview](./ARCHITECTURE.md)
- [Public Crates](./PUBLIC_CRATES.md)
- [Development Guide](./DEVELOPMENT.md)
- [Contributing Guide](./CONTRIBUTING.md)

---

**Last Updated**: October 2025  
**Maintained by**: ForgeDB Team
