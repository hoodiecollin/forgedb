# ForgeDB Schema Language Support

Syntax highlighting and language support for ForgeDB schema files (`.forge`).

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

1. Install the extension
2. Create a `.forge` file in your project
3. Start typing - syntax highlighting will activate automatically
4. Use snippets (type `model`, `fstring`, etc. and press Tab)

## Requirements

- VSCode 1.80.0 or higher

## Extension Settings

This extension provides the following default settings for `.forge` files:
- Tab size: 2 spaces
- Insert spaces: Enabled
- Quick suggestions: Enabled

## Keyboard Shortcuts

- `Cmd+/` (Mac) or `Ctrl+/` (Windows/Linux): Toggle line comment
- `Cmd+K Cmd+C`: Add line comment
- `Cmd+K Cmd+U`: Remove line comment
- `Shift+Alt+A`: Toggle block comment

## Known Limitations

- No LSP support (coming in Sprint 22)
- No diagnostics/error checking (coming in Sprint 22)
- No code completion beyond snippets (coming in Sprint 22)
- No go-to-definition (coming in Sprint 22)

## Roadmap

- ✅ **Sprint 21**: Syntax highlighting (current)
- 🔜 **Sprint 22**: Language Server Protocol (LSP)
  - Real-time diagnostics
  - Intelligent code completion
  - Hover information
  - Go to definition
  - Rename refactoring
- 🔜 **Sprint 23**: Full VSCode Extension
  - Integrated commands
  - Code generation
  - Schema validation
  - File watcher integration

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
