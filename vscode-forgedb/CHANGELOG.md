# Change Log

All notable changes to the "ForgeDB Schema Language" extension will be documented in this file.

## [0.1.0] - 2025-10-14

### Added
- Initial release
- Syntax highlighting for `.forge` files
- TextMate grammar with support for:
  - Keywords (struct, model names)
  - Data types (string, u32, i64, uuid, timestamp, etc.)
  - Field modifiers (+, &, ^, *, ?)
  - Directives (@email, @min, @max, @index, etc.)
  - Relations ([Model], *Model)
  - Component references (tsx://, jsx://, api://)
  - Comments (line and block)
- Language configuration:
  - Auto-closing pairs
  - Bracket matching
  - Comment toggling
  - Smart indentation
  - Code folding
- 30+ code snippets for:
  - Model templates
  - Field types
  - Directives
  - Common patterns
- Editor defaults (2-space tabs, smart suggestions)

### Features Planned
- LSP integration (Sprint 22)
- Commands for code generation (Sprint 23)
- Real-time diagnostics (Sprint 23)
- Schema validation (Sprint 23)
