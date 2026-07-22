# ForgeDB VSCode Extension

IDE support for ForgeDB schema files (`.forge`): syntax highlighting, Language Server Protocol (LSP), and integrated commands.

## Features

### 🎨 Syntax Highlighting
- **Keywords**: `struct`, `enum`, model names
- **Types**: `string`, `u32`, `i64`, `f64`, `bool`, `uuid`, `timestamp`, `decimal`, `json`, enums, etc.
- **Symbols**: `+` (auto-generate), `&` (unique), `^` (index), `*` (required relation), `?` (optional/nullable)
- **Directives**: `@email`, `@url`, `@min`, `@max`, `@computed`, `@index`, `@fulltext`, etc.
- **Relations**: `[Model]` (one-to-many / many-to-many), `*Model` (required FK), `?Model` (optional FK)
- **Comments**: `//` (line comments only)
- **Component References**: `tsx://`, `jsx://`, `api://`

### 📝 Code Snippets

Speed up schema authoring with intelligent snippets:

**Model Templates:**
- `model` - Basic model with common fields
- `modelrel` - Model with relations
- `tuser` - Complete User model template
- `tpost` - Blog post model template
- `tcomment` - Comment model template

**Field Snippets:**
- `fid` - UUID primary key
- `fidauto` - Auto-increment u64 primary key
- `femail` - Unique, indexed email field with validation
- `fstring` - String field
- `fstringopt` - Nullable string field
- `fstringuniq` - Unique string field (`&`)
- `fstringidx` - Indexed string field (`^`)
- `fbool` - Boolean field
- `fnum` - Numeric field
- `fdecimal` - Exact decimal field
- `fjson` - JSON field
- `fchar` - Fixed-length char field
- `ftimestamp` - Timestamp field
- `farray` - One-to-many / many-to-many relation
- `frel` - Required relation (`*Model`)
- `frelopt` - Optional relation (`?Model`)
- `fcomputed` - Computed field
- `fminmax` - Numeric field with `@min`/`@max`
- `flength` - String field with `@length`
- `fpattern` - String field with `@pattern`
- `fcomponent` - Component reference
- `fapi` - API route handler

**Directive Snippets:**
- `dindex` - Composite index
- `ddefault` - Default value
- `dondel` - On delete behavior
- `dfulltext` - Full-text search
- `dsoftdelete` - Model-level soft delete

### 🔧 Editor Features

- **Auto-closing pairs**: Brackets, quotes, and parentheses
- **Bracket matching**: Highlight matching `{}`, `[]`, `()`
- **Comment toggling**: Line comments (`//`)
- **Smart indentation**: Auto-indent inside blocks
- **Code folding**: Collapse model and struct definitions

### 🧠 Language Server (LSP)

- **Real-time Diagnostics**: Syntax errors, type checking, schema validation
- **Code Completion**: Context-aware suggestions for types, directives, modifiers
- **Hover Information**: Documentation for types, directives, and models
- **Go to Definition**: Navigate to model and struct definitions
- **Rename Refactoring**: Rename models/fields with automatic reference updates

### ⚡ Commands

Access via Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`):

- **ForgeDB: Generate Code** - Run code generation from schema
- **ForgeDB: Validate Schema** - Validate current schema file
- **ForgeDB: Start Dev Mode** - Start file watcher for auto-generation
- **ForgeDB: Create New Model** - Interactive model creation wizard
- **ForgeDB: Restart Language Server** - Restart LSP server
- **ForgeDB: Show Output** - Show LSP server output

### 📊 Status Bar

Real-time ForgeDB status indicator showing:
- Extension active/inactive state
- Schema validation status
- Quick access to commands

## Example Schema

```forge
// User model with authentication
User {
  id: +uuid
  email: ^&string @email
  username: ^&string @length(3, 50)
  password_hash: string
  full_name: string?
  avatar_url: string? @url
  is_active: bool @default(true)

  // Relations
  posts: [Post]
  comments: [Comment]

  // Component references
  card: tsx://pages/user/card @relations(posts)
  profile: tsx://pages/user/profile @relations(*)

  // API routes
  verify_email: api://routes/user/verify

  created_at: timestamp
  updated_at: timestamp
}

// Blog post with full-text search
Post {
  id: +uuid
  title: string @length(1, 200) @fulltext
  slug: ^&string
  content: string @fulltext
  published: bool @default(false)

  // Relations
  author: *User @on_delete(cascade)
  comments: [Comment]
  tags: [Tag]

  created_at: timestamp
  updated_at: timestamp
}

// Inline struct for address
struct Address {
  street: char(100)
  city: char(50)
  state: char(2)
  zip: char(10)
}
```

## Supported Syntax

### Field Modifiers
- `+` - Auto-generate (`u32`/`u64`/`uuid`/`timestamp` only)
- `&` - Unique
- `^` - Index (ordered → range queries for sortable types)
- `*` - Required relation reference
- `?` - Optional (nullable), or optional relation reference

### Data Types

**Numeric:**
- `u32`, `u64` - Unsigned integers
- `i32`, `i64` - Signed integers
- `f64` - Floating point
- `decimal` - Exact fixed-point (`rust_decimal::Decimal`)

**Text:**
- `string` - Variable-length string
- `char(n)` - Fixed-length character array

**Other:**
- `bool` - Boolean
- `uuid` - UUID v4
- `timestamp` - Unix timestamp (i64)
- `json` - Arbitrary JSON (`serde_json::Value`)
- `EnumName` - Reference to a top-level `enum Name { ... }`

### Directives

**Validation:**
- `@email` - Email format validation
- `@url` - URL format validation
- `@min(n)` - Minimum value (numeric fields only)
- `@max(n)` - Maximum value (numeric fields only; use `@length` for strings)
- `@pattern("…")` / `@regex("…")` - Regex validation (string)
- `@length(n)` or `@length(min, max)` - String length

**Database / relations:**
- `&` modifier - Unique constraint (uniqueness is a modifier, not a `@unique` directive)
- `@index(field1, field2, ...)` - Composite index (model level)
- `@fulltext` - Full-text search marker (**semantic-only**; no index generated)
- `@default(value)` - Default value marker (**semantic-only**; not applied at write)
- `@on_delete(cascade|set_null|restrict)` - Foreign-key on-delete policy (enforced)

**Computed:**
- `@computed` - Computed field (not stored)

**UI/API:**
- `@relations(field1, field2, ...)` - Include relations in component props
- `@relations(*)` - Include all relations

## Getting Started

### Installation

1. Install the extension from the VS Code marketplace.
2. Install the **`forgedb` CLI** so it is on your `PATH` — the extension launches
   the language server through it (`forgedb lsp`), so no separate server download
   is needed. See the [install guide](https://github.com/hoodiecollin/forgedb#installation).
3. Open or create a `.forge` file — the extension activates automatically.

If `forgedb` is installed somewhere not on `PATH`, set `forgedb.path` in settings.
For LSP development, point `forgedb.lspServerPath` at a local
`target/debug/forgedb-lsp` build.

### First Steps

1. Create a schema file: `schema.forge`
2. Start typing - get instant syntax highlighting and completions
3. Use snippets: type `model`, `fstring`, etc. and press Tab
4. Save to see real-time diagnostics
5. Run commands from Command Palette

## Requirements

- VS Code 1.80.0 or higher
- The `forgedb` CLI installed and on `PATH` (it carries the language server)

## Extension Settings

Configure via Settings (`Cmd+,` / `Ctrl+,`) or `settings.json`:

```json
{
  // Path to the `forgedb` CLI (empty = resolve from PATH).
  "forgedb.path": "",

  // Explicit `forgedb-lsp` server binary; overrides forgedb.path.
  // Empty = resolve from the installed CLI. Use for LSP development.
  "forgedb.lspServerPath": "",

  // Run `forgedb generate` automatically when a .forge file changes.
  "forgedb.autoGenerateOnSave": false
}
```

## Building & packaging (from source)

All commands run from the repository root — no `cd` required:

```bash
make extension-build      # compile src/extension.ts -> out/extension.js
make extension-typecheck  # type-check without emitting
make extension-package    # produce an installable apps/vscode-forgedb/forgedb-*.vsix
```

## Keyboard Shortcuts

- `Cmd+/` (Mac) or `Ctrl+/` (Windows/Linux): Toggle line comment
- `Cmd+K Cmd+C`: Add line comment
- `Cmd+K Cmd+U`: Remove line comment

## Architecture

The extension integrates three main components:

1. **TextMate Grammar**: Syntax highlighting engine
2. **Language Server**: Rust-based LSP server for diagnostics and completion
3. **Extension Client**: TypeScript client with commands and UI integration

```
┌─────────────────────────────────────┐
│   VSCode Extension (TypeScript)     │
│  ┌───────────┐  ┌────────────────┐  │
│  │ Commands  │  │  Status Bar    │  │
│  └───────────┘  └────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │   Language Client (LSP)       │  │
│  └───────────┬───────────────────┘  │
└──────────────┼──────────────────────┘
               │ JSON-RPC
┌──────────────┴──────────────────────┐
│  Language Server (Rust)             │
│  ┌──────────┐  ┌────────────────┐   │
│  │  Parser  │  │  Diagnostics   │   │
│  └──────────┘  └────────────────┘   │
│  ┌──────────┐  ┌────────────────┐   │
│  │Completion│  │     Hover      │   │
│  └──────────┘  └────────────────┘   │
└─────────────────────────────────────┘
```

## Troubleshooting

### LSP Server Not Starting

If you see a warning that the ForgeDB CLI was not found:

1. Confirm the CLI is installed and on `PATH`: `forgedb --version`
2. If it lives elsewhere, set `"forgedb.path": "/path/to/forgedb"`
3. For LSP development from source, build the server
   (`cargo build -p forgedb-lsp-server`) and point
   `"forgedb.lspServerPath": "/path/to/target/debug/forgedb-lsp"`

### No Diagnostics Appearing

1. Ensure the file is saved (`.forge` extension)
2. Check the Output panel: **ForgeDB: Show Output** command
3. Restart the server: **ForgeDB: Restart Language Server**

### Commands Not Working

1. Ensure the `forgedb` CLI is installed and on `PATH` (or set `forgedb.path`)
2. Open the integrated terminal to see the command's output

## Development

To develop this extension:

```bash
# Install dependencies
cd vscode-forgedb
npm install

# Compile TypeScript
npm run compile

# Watch for changes
npm run watch

# Package extension
npm run package
```

## Roadmap

- ✅ Syntax highlighting
- ✅ Language Server Protocol (LSP)
- ✅ Full VSCode Extension
- 🔜 **Future**: Marketplace publishing, additional commands

## Contributing

ForgeDB is open source! Contributions welcome.

## License

MIT License - See LICENSE file for details

## Support

- 📖 [Documentation](https://forgedb.dev/docs)
- 🐛 [Report Issues](https://github.com/forgedb/forgedb/issues)
- 💬 [Discussions](https://github.com/forgedb/forgedb/discussions)

---

**Enjoy building with ForgeDB!** 🔨⚡
