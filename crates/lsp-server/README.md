# ForgeDB Language Server

Language Server Protocol (LSP) implementation for ForgeDB schema files (`.forge`).

The server **consumes the real ForgeDB compiler** rather than reimplementing the
grammar. Buffers are parsed with `forgedb_parser::Parser::parse_recover` — a
resilient partial parse that returns a best-effort AST plus every diagnostic the
compiler would emit (recovered syntax errors merged with the semantic errors from
`forgedb_parser::validate_schema`). As a result, what the editor shows tracks the
language exactly and matches `forgedb validate`. There is no private schema parser.

## Features

### Real-time Diagnostics
Positioned syntax + semantic errors sourced straight from the compiler
(`parse_recover().diagnostics`), mapped 1-based → 0-based to LSP ranges. Because
the diagnostics come from the compiler, the editor never invents or misses a rule
relative to `forgedb validate`.

### Code Completion
- Field-type suggestions for the actual scalar set: `string`, `bool`, `u32`, `u64`,
  `i32`, `i64`, `f64`, `decimal`, `json`, `uuid`, `timestamp`, `char(N)`.
- Field-modifier suggestions: `+` (auto-generate), `&` (unique), `^` (index),
  `*` (required FK), `?` (optional). There is no `~` modifier.
- Directive suggestions (`@email`, `@min`, `@max`, `@pattern`, `@on_delete`, …).
- Model, struct, and **enum** reference completion, read from the parsed AST.

### Hover Information
- Type and directive documentation aligned to the real grammar.
- Model / struct / enum structure (fields or variants) on hover.
- Modifier explanations.

### Go to Definition
Jump to model, struct, or enum definitions (positions come from the AST's
`Option<Position>` nodes).

### Rename Refactoring
Whole-word, buffer-wide reference rename.

## Architecture

```
forgedb-lsp-server/
├── src/
│   ├── lib.rs            # the whole server: tower-lsp event loop + Backend + run()
│   ├── diagnostics.rs    # thin mapper: compiler ValidationError -> LSP Diagnostic
│   ├── completion.rs     # code completion (grammar-driven + AST references)
│   └── hover.rs          # hover information
└── Cargo.toml            # depends on forgedb-parser + forgedb-validation
```

This crate is **library-only** — it exposes `run()` (which owns a Tokio runtime
and serves LSP over stdio) plus the reusable diagnostic mapper. The shipped
`forgedb-lsp` *binary* lives in the root `forgedb` crate behind its non-default
`lsp` feature, so one distribution "app" carries both `forgedb` and `forgedb-lsp`
(epic #173). The library split also lets the diagnostic mapper be exercised
outside the LSP event loop: the root crate's `tests/lsp_cli_parity.rs` fixture
imports `to_lsp_diagnostics` and asserts the editor and `forgedb validate` emit
the same diagnostic set over `examples/*` (epic #173).

## Usage

### Build
```bash
# The bundled server binary is built from the root crate under the `lsp` feature:
cargo build --release --features lsp --bin forgedb-lsp
```

### Run
```bash
cargo run --features lsp --bin forgedb-lsp
# ...or, equivalently, through the CLI launcher:
cargo run -- lsp
```

The server communicates via stdin/stdout using the LSP protocol.

## Integration with VSCode

See `apps/vscode-forgedb/` extension for client integration.

## LSP Capabilities

- `textDocument/didOpen` - Document opened
- `textDocument/didChange` - Document changed
- `textDocument/didSave` - Document saved
- `textDocument/completion` - Code completion
- `textDocument/hover` - Hover information
- `textDocument/definition` - Go to definition
- `textDocument/rename` - Rename symbol
- `textDocument/publishDiagnostics` - Real-time diagnostics

## Testing

```bash
cargo test -p forgedb-lsp-server   # unit tests for the server + helpers
```

## Future Enhancements

- Code actions (quick fixes)
- Semantic tokens
- Document / workspace symbols
- Signature help
- Document formatting
- Multi-file / import handling

## Documentation

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow

## License

Part of the ForgeDB project.
