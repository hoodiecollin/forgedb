# ForgeDB Language Server

Language Server Protocol (LSP) implementation for ForgeDB schema files (`.forge`).

## Features

### ✅ Real-time Diagnostics
- Syntax error detection
- Schema validation
- Type checking
- Duplicate field detection
- Missing primary key warnings
- Undefined model references
- Invalid directive usage
- Constraint validation

### ✅ Code Completion
- Field type suggestions (string, u32, uuid, etc.)
- Field modifier suggestions (+, &, ^, *, ?)
- Directive suggestions (@email, @min, @max, etc.)
- Model reference completion
- Context-aware suggestions

### ✅ Hover Information
- Type documentation
- Directive documentation
- Model structure on hover
- Field information
- Modifier explanations

### ✅ Go to Definition
- Jump to model definitions
- Jump to struct definitions
- Navigate relation references

### ✅ Rename Refactoring
- Rename models (updates all references)
- Rename fields (updates all references)
- Whole-word matching
- Multi-file support

## Architecture

```
forgedb-lsp-server/
├── src/
│   ├── main.rs           # LSP server implementation
│   ├── parser.rs         # Schema parser & AST
│   ├── diagnostics.rs    # Validation & error checking
│   ├── completion.rs     # Code completion
│   └── hover.rs          # Hover information
└── Cargo.toml
```

## Usage

### Build
```bash
cargo build --release -p forgedb-lsp-server
```

### Run
```bash
cargo run --bin forgedb-lsp
```

The server communicates via stdin/stdout using the LSP protocol.

## Integration with VSCode

See `vscode-forgedb/` extension for client integration.

## LSP Capabilities

- `textDocument/didOpen` - Document opened
- `textDocument/didChange` - Document changed
- `textDocument/didSave` - Document saved
- `textDocument/completion` - Code completion
- `textDocument/hover` - Hover information
- `textDocument/definition` - Go to definition
- `textDocument/rename` - Rename symbol
- `textDocument/publishDiagnostics` - Real-time diagnostics

## Parser

The parser (`parser.rs`) provides an AST representation of ForgeDB schemas:

```rust
pub struct Schema {
    pub models: Vec<Model>,
    pub structs: Vec<Struct>,
}

pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub position: Position,
}

pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub modifiers: Vec<FieldModifier>,
    pub directives: Vec<Directive>,
    pub position: Position,
}
```

## Diagnostics

Validates:
- Duplicate field names
- Missing primary keys
- Undefined model references
- Invalid modifier combinations
- Directive usage and arguments
- Type constraints

## Testing

```bash
cargo test -p forgedb-lsp-server
```

## Future Enhancements

- Code actions (quick fixes)
- Semantic tokens
- Document symbols
- Workspace symbols
- Signature help
- Document formatting
- Incremental parsing
- Multi-file support
- Import/export handling
