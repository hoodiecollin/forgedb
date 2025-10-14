# ForgeDB VSCode Extension

Complete IDE support for ForgeDB schema files (`.forge`) with syntax highlighting, Language Server Protocol (LSP), and integrated commands.

## Features

### 🎨 Syntax Highlighting
- **Keywords**: `struct`, model names
- **Types**: `string`, `u32`, `i64`, `f64`, `bool`, `uuid`, `timestamp`, etc.
- **Symbols**: `+` (primary key), `&` (required), `^` (unique), `*` (relation), `?` (optional)
- **Directives**: `@email`, `@url`, `@min`, `@max`, `@computed`, `@index`, `@fulltext`, etc.
- **Relations**: `[Model]` (array), `*Model` (single)
- **Comments**: `//` (line) and `/* */` (block)
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
- `femail` - Email field with validation
- `fstring` - Required string field
- `fstringopt` - Optional string field
- `fstringuniq` - Unique string field
- `fbool` - Boolean field
- `fnum` - Numeric field
- `ftimestamp` - Timestamp field
- `farray` - Array relation
- `frel` - Single relation
- `fcomputed` - Computed field
- `fcomponent` - Component reference
- `fapi` - API route handler

**Directive Snippets:**
- `dindex` - Composite index
- `dunique` - Unique constraint
- `ddefault` - Default value
- `dondel` - On delete behavior
- `dfulltext` - Full-text search

### 🔧 Editor Features

- **Auto-closing pairs**: Brackets, quotes, and parentheses
- **Bracket matching**: Highlight matching `{}`, `[]`, `()`
- **Comment toggling**: Line and block comments
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
  username: ^&string @min(3) @max(50)
  password_hash: &string
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
  title: &string @max(200) @fulltext
  slug: ^&string
  content: &string @fulltext
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
- `+` - Primary key (auto-generated or auto-increment)
- `&` - Required (non-nullable)
- `^` - Unique constraint
- `*` - Relation reference
- `?` - Optional (nullable)

### Data Types

**Numeric:**
- `u8`, `u16`, `u32`, `u64` - Unsigned integers
- `i8`, `i16`, `i32`, `i64` - Signed integers
- `f32`, `f64` - Floating point

**Text:**
- `string` - Variable-length string
- `char(n)` - Fixed-length string

**Other:**
- `bool` - Boolean
- `uuid` - UUID v4
- `timestamp` - Unix timestamp (i64)

### Directives

**Validation:**
- `@email` - Email format validation
- `@url` - URL format validation
- `@min(n)` - Minimum value/length
- `@max(n)` - Maximum value/length
- `@regex(pattern)` - Regex validation
- `@length(n)` - Exact length

**Database:**
- `@unique` - Unique constraint
- `@index(field1, field2, ...)` - Composite index
- `@fulltext` - Full-text search index
- `@default(value)` - Default value
- `@on_delete(cascade|set_null|restrict)` - Foreign key behavior

**Computed:**
- `@computed` - Computed field (not stored)

**UI/API:**
- `@relations(field1, field2, ...)` - Include relations in component props
- `@relations(*)` - Include all relations

## Getting Started

### Installation

1. Install the extension from VSCode marketplace
2. Ensure ForgeDB is installed in your project
3. Build the LSP server: `cargo build -p forgedb-lsp-server`
4. Open or create a `.forge` file - the extension activates automatically

### First Steps

1. Create a schema file: `schema.forge`
2. Start typing - get instant syntax highlighting and completions
3. Use snippets: type `model`, `fstring`, etc. and press Tab
4. Save to see real-time diagnostics
5. Run commands from Command Palette

## Requirements

- VSCode 1.80.0 or higher
- ForgeDB project with LSP server built (`cargo build -p forgedb-lsp-server`)

## Extension Settings

Configure via Settings (`Cmd+,` / `Ctrl+,`) or `settings.json`:

```json
{
  // Path to schema file
  "forgedb.schemaPath": "schema.forge",

  // Output directory for generated code
  "forgedb.outputDirectory": "generated",

  // Auto-generate on save
  "forgedb.autoGenerateOnSave": false,

  // Custom LSP server path (auto-detected if empty)
  "forgedb.lspServerPath": "",

  // LSP trace level (off, messages, verbose)
  "forgedb.trace.server": "off"
}
```

### Default Editor Settings

The extension configures optimal settings for `.forge` files:
- Tab size: 2 spaces
- Insert spaces: Enabled
- Quick suggestions: Enabled for code (disabled in comments/strings)

## Keyboard Shortcuts

- `Cmd+/` (Mac) or `Ctrl+/` (Windows/Linux): Toggle line comment
- `Cmd+K Cmd+C`: Add line comment
- `Cmd+K Cmd+U`: Remove line comment
- `Shift+Alt+A`: Toggle block comment

## Architecture

The extension integrates three main components:

1. **TextMate Grammar** (Sprint 21): Syntax highlighting engine
2. **Language Server** (Sprint 22): Rust-based LSP server for diagnostics and completion
3. **Extension Client** (Sprint 23): TypeScript client with commands and UI integration

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

If you see warnings about LSP server not found:

1. Build the server: `cargo build -p forgedb-lsp-server`
2. Check the binary exists: `target/debug/forgedb-lsp` or `target/release/forgedb-lsp`
3. Set custom path in settings: `"forgedb.lspServerPath": "/path/to/forgedb-lsp"`

### No Diagnostics Appearing

1. Ensure file is saved (`.forge` extension)
2. Check Output panel: **ForgeDB: Show Output** command
3. Enable trace logging: `"forgedb.trace.server": "verbose"`

### Commands Not Working

1. Ensure you're in a ForgeDB project
2. Check that `cargo` is in your PATH
3. Verify workspace has ForgeDB CLI installed

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

- ✅ **Sprint 21**: Syntax highlighting
- ✅ **Sprint 22**: Language Server Protocol (LSP)
- ✅ **Sprint 23**: Full VSCode Extension (current)
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
