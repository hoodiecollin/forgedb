# Contributing to ForgeDB

Thank you for your interest in contributing to ForgeDB! This guide will help you get started.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Testing Requirements](#testing-requirements)
- [Pull Request Process](#pull-request-process)
- [Code Style](#code-style)
- [Documentation Requirements](#documentation-requirements)
- [Community](#community)

---

## Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inspiring community for all. Please be respectful and constructive in all interactions.

### Our Standards

**Positive behavior includes:**
- Using welcoming and inclusive language
- Being respectful of differing viewpoints
- Gracefully accepting constructive criticism
- Focusing on what is best for the community
- Showing empathy towards others

**Unacceptable behavior includes:**
- Harassment or discriminatory language
- Trolling, insulting comments, or personal attacks
- Public or private harassment
- Publishing others' private information
- Other conduct inappropriate in a professional setting

### Enforcement

Project maintainers have the right to remove, edit, or reject comments, commits, code, issues, and other contributions that do not align with this Code of Conduct.

Report unacceptable behavior by opening a confidential report through
[GitHub's private reporting](https://github.com/hoodiecollin/forgedb/security/advisories) or by
contacting the maintainers via a GitHub issue.

---

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check existing issues to avoid duplicates.

**When filing a bug report, include:**
- Clear, descriptive title
- Exact steps to reproduce
- Expected vs. actual behavior
- ForgeDB version (`forgedb --version`)
- Operating system and version
- Rust version (`rustc --version`)
- Relevant schema files (if applicable)
- Error messages or logs

**Bug Report Template:**
```markdown
### Description
[Clear description of the bug]

### Steps to Reproduce
1. Create schema with...
2. Run command...
3. Observe error...

### Expected Behavior
[What should happen]

### Actual Behavior
[What actually happens]

### Environment
- ForgeDB version: 0.1.0
- OS: Ubuntu 22.04
- Rust: 1.70.0

### Additional Context
[Logs, screenshots, etc.]
```

### Suggesting Enhancements

**Enhancement suggestions should include:**
- Clear, descriptive title
- Detailed description of proposed functionality
- Use cases and benefits
- Examples of how it would work
- Potential drawbacks or alternatives

**Enhancement Template:**
```markdown
### Feature Description
[Clear description]

### Motivation
[Why is this needed?]

### Proposed Solution
[How should it work?]

### Examples
```forgedb
// Example schema or code
```

### Alternatives Considered
[Other approaches]

### Additional Context
[Links, references, etc.]
```

**Design notes & proposals live as issues, not committed files.** A non-trivial design is captured
as the **design gate** — a sub-issue of the work item it belongs to, labelled
`improvement:gate-1` — and we do **not** commit proposal/design documents to the repository.
Durable *architecture* reference for shipped features belongs in
[`ARCHITECTURE.md`](./ARCHITECTURE.md); the gate issue holds the forward-looking design while it is
under discussion, and closing it means the design was accepted. This keeps the tree free of
point-in-time design notes that drift out of sync with the code.

### Your First Code Contribution

**Good first issues** are labeled `good-first-issue` on GitHub. These are:
- Well-defined and scoped
- Have clear acceptance criteria
- Don't require deep system knowledge

**Areas needing help:**
- Documentation improvements
- Test coverage
- Bug fixes
- Code examples
- Performance optimizations

### Pull Requests

See [Pull Request Process](#pull-request-process) below for detailed guidelines.

---

## Development Setup

### Prerequisites

**Required:**
- Rust 1.96 — pinned in `rust-toolchain.toml`; rustup honours it automatically ([rustup.rs](https://rustup.rs/)).
  The workspace is edition 2024, which requires ≥ 1.85; 1.70 cannot build it.
- **Go** ([go.dev/dl](https://go.dev/dl/)) — required to run the test suite, not optional.
  `tools/goguard` parses generated Go through `go/parser` so the identity red line is checked
  against an AST rather than five substrings. A missing toolchain **fails** the Go guards; it
  never skips them, because a guard that skips reports green without having evaluated.
- Git

**Optional but recommended:**
- VSCode with rust-analyzer extension
- Cargo watch (`cargo install cargo-watch`)

### Clone Repository

```bash
git clone https://github.com/yourusername/forgedb.git
cd forgedb
```

### Build Project

```bash
# Build all crates
cargo build

# Build specific crate
cargo build --package forgedb-parser

# Build with optimizations
cargo build --release
```

### Run Tests

```bash
# Run all tests
cargo test --lib

# Run tests for specific crate
cargo test --package forgedb-storage

# Run specific test
cargo test test_parse_model --package forgedb-parser

# Run with output
cargo test -- --nocapture
```

### Run Examples

```bash
# List examples
ls examples/

# Run example
cargo run -- generate all --output ./generated
```

### Development Tools

**Cargo Watch** (auto-rebuild on changes):
```bash
cargo install cargo-watch
cargo watch -x build
cargo watch -x test
```

**Clippy** (linter):
```bash
cargo clippy --all-targets --all-features
```

**Rustfmt** (formatter):
```bash
cargo fmt --all
```

**Documentation**:
```bash
cargo doc --open
```

### IDE Setup

**VSCode** (`.vscode/settings.json`):
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

---

## Testing Requirements

### Test Coverage Requirements

All contributions must include tests:

**For new features:**
- Unit tests for individual functions
- Integration tests for workflows
- Documentation tests (doctests)
- Test coverage ≥ 80%

**For bug fixes:**
- Regression test that fails without the fix
- Verify fix resolves the issue

### Writing Tests

**Unit Tests:**
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
        
        let result = parse_schema(input);
        assert!(result.is_ok());
        
        let schema = result.unwrap();
        assert_eq!(schema.models.len(), 1);
        assert_eq!(schema.models[0].name, "User");
        assert_eq!(schema.models[0].fields.len(), 2);
    }
    
    #[test]
    fn test_parse_invalid_syntax() {
        let input = "User { id: invalidtype }";
        let result = parse_schema(input);
        assert!(result.is_err());
    }
}
```

**Integration Tests** (`tests/` directory):
```rust
use forgedb_parser::parse_schema;
use forgedb_validation::validate_schema;

#[test]
fn test_full_validation_pipeline() {
    let input = include_str!("fixtures/valid_schema.forge");
    
    let schema = parse_schema(input).expect("Parse failed");
    let errors = validate_schema(&schema);
    
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);
}
```

**Documentation Tests:**
```rust
/// Parse a ForgeDB schema from string.
///
/// # Examples
///
/// ```
/// use forgedb_parser::parse_schema;
///
/// let schema = parse_schema(r#"
///     User {
///       id: +uuid
///       email: string
///     }
/// "#).unwrap();
///
/// assert_eq!(schema.models.len(), 1);
/// ```
pub fn parse_schema(input: &str) -> Result<Schema> {
    // Implementation
}
```

### Running Specific Test Categories

```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test '*'

# Doc tests only
cargo test --doc

# Specific crate
cargo test --package forgedb-parser

# With code coverage (requires cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Test Organization

```
crate-name/
├── src/
│   ├── lib.rs           # Unit tests in mod tests { }
│   └── parser.rs        # Unit tests in mod tests { }
├── tests/
│   ├── integration_test.rs
│   └── fixtures/
│       └── test_schema.forge
└── Cargo.toml
```

---

## Pull Request Process

### Which branch does your PR target?

ForgeDB keeps two long-lived branches, because generated code links the
`forgedb-*` substrate crates **from crates.io** and those are published once per
release rather than per change. Between the change and the publish there is a
window where the repo builds fine in-tree but an installed user cannot build at
all. That window is held off the default branch.

| Branch | What it holds | Guarantee |
|---|---|---|
| `main` | released state | Always releasable — an outside-repo `forgedb init → generate → cargo build` resolves entirely from crates.io. |
| `develop` | the current release cycle | May carry unpublished substrate APIs. |

**Core changes** — anything under `crates/`, `src/`, `tests/`, `examples/` —
branch from `develop` and target `develop`:

```bash
git checkout develop && git pull
git checkout -b feature/my-new-feature   # or fix/… , perf/… , refactor/…
```

> `main` remains the repository's default branch, so that cloning gives you a tree
> that actually builds and installs. That means **GitHub will pre-fill `main` as
> your PR's base** — change it to `develop` for core work. A core PR merged into
> `main` puts an unpublished substrate API on the branch that is supposed to be
> releasable, which is precisely what this split exists to prevent.

**Docs, website, and extension changes** are decided by *coupling*, not by which
directory they live in. Ask: **does this change describe, depend on, or
demonstrate behavior that is not released yet?**

- **No → target `main`.** Typo and link fixes, styling, SEO, analytics, dependency
  bumps, corrections to already-shipped documentation. These deploy continuously
  and should not wait on a release they have nothing to do with.
- **Yes → target `develop`, in the same PR as the feature.** Documentation for an
  unreleased feature, examples calling an unreleased API, a schema reference for
  syntax that does not parse yet.

Documentation that lands ahead of the feature it documents is worse than no
documentation: it describes an API nobody can call, and it makes the docs
untrustworthy exactly when someone is relying on them. Pair feature docs with the
feature.

Releases go **publish the substrate → merge `develop` into `main` → tag**, in that
order. The `Substrate reclose` workflow runs on `main` and proves the first step
actually happened.

### There is exactly one `develop`

No `v0.5-develop` alongside a `v0.4-develop`. The branch name never contains a
version, for two reasons.

crates.io has **one version line per crate**, and the publish gap is defined
relative to what is currently published — so two cycle branches carrying
unpublished substrate changes cannot both be measured against it. Whichever
publishes first silently redefines the other's gap. Separately, the **milestone
already says when a change ships**; putting a version in the branch name encodes
that a second time, somewhere harder to query and harder to correct.

Keeping it version-agnostic also makes it self-advancing: `develop` means "the
cycle in flight," so tagging a release turns it into the next cycle with no
rename and no workflow edit.

**So what keeps next-cycle work off `develop` is the milestone, not the branch:**

> A PR targeting `develop` may not close an issue milestoned later than the cycle
> in flight (the lowest open `v*` milestone).

Note the shape — a deny-list on *future* milestones, not an allow-list on the
current one. Chores, CI fixes and typo PRs close no issue and pass silently, and
correctly so: work with no issue cannot be next-cycle work, because next-cycle
work is *defined* by carrying that milestone.

The `Cycle scope` workflow enforces this on PRs to `develop`. Most work here
merges locally, so run the same check yourself before merging a branch back:

```bash
make cycle-scope ISSUE=245        # the issue(s) your branch closes
make cycle-scope PR=250           # or a PR, as CI does
```

If it blocks you, the work is not wrong — it is early. Keep the branch, let
`develop` ship its cycle, then rebase and land it. The only other correct
response is that the issue was mis-scheduled, in which case move the milestone
rather than merging past it.

**Work scheduled for a later version** therefore lives on its own branch off
`develop`, unmerged, carrying its real milestone. Two long-lived branches *are*
legitimate and neither is a second cycle line: a maintenance line cut off a tag
(`release/v0.4.x`) when a patch is needed after the cycle moved on, and a track
that cannot merge into the current cycle at all — named for the work
(`format-v2`), never for a version, so nobody reads it as a release line.

### Before Submitting

**1. Create an issue** (if one doesn't exist) describing the change.

**2. Fork and create a branch** from the base chosen above:
```bash
git checkout -b feature/my-new-feature
# or
git checkout -b fix/bug-description
```

**3. Make your changes** following [Code Style](#code-style).

**4. Add tests** covering your changes.

**5. Run checks:**
```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all

# Build documentation
cargo doc --no-deps
```

**6. Update documentation** (see [Documentation Requirements](#documentation-requirements)).

**7. Commit changes** with clear messages:
```bash
git add .
git commit -m "feat: Add full-text search support

- Implement tokenization
- Add inverted index
- Create search query parser
- Add integration tests

Closes #123"
```

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style (formatting, semicolons, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(parser): Add support for enum types

Implements enum parsing in schema language.

Closes #45

---

fix(storage): Correct off-by-one error in column indexing

The column index was incorrectly calculated when accessing
variable-length columns, causing data corruption.

Fixes #78

---

docs(contributing): Update testing requirements

Clarify coverage requirements and add examples.
```

### Submitting Pull Request

**1. Push to your fork:**
```bash
git push origin feature/my-new-feature
```

**2. Create PR on GitHub** with:

**PR Title**: Clear, descriptive (like commit message)

**PR Description Template**:
```markdown
## Description
[What does this PR do?]

## Motivation
[Why is this change needed?]

## Changes
- [ ] Feature 1
- [ ] Feature 2
- [ ] Tests added
- [ ] Documentation updated

## Testing
[How was this tested?]

## Checklist
- [ ] Code follows style guidelines
- [ ] Self-reviewed code
- [ ] Commented complex code
- [ ] Updated documentation
- [ ] No new warnings
- [ ] Added tests
- [ ] All tests pass
- [ ] Updated CHANGELOG.md

## Related Issues
Closes #123
```

### Review Process

**1. Automated checks** run (CI):

| Check | Runs on | Blocks? |
|---|---|---|
| **`test suite`** — tier 1 (`make test`) | PRs into `develop`/`main`, and pushes to both | **Yes**, on PRs |
| Cycle-scope gate | PRs into `develop` | Yes |
| Substrate outside-repo reclose | `main` | Yes |
| Reclose under a foreign cargo workspace root | `main` | Yes |
| `migrate_tests` vs the published substrate | `main` | Yes |
| Go reclose | `main` | Yes |
| Nightly ignored suite — tier 2 | schedule, against `develop` | Files an issue |

Run tier 1 yourself before opening a PR — it is the same command CI runs:

```bash
make test          # cargo test --workspace --no-fail-fast + cargo build --workspace --examples
```

The examples build is not optional: `--lib`, `--bins`, `--tests` **and** `--doc` all
exclude examples, so no test flag covers them, and their omission has silently broken
the tree twice.

The slow tier is the ~20 tests that each generate and compile a crate. They are
`#[ignore]`d out of tier 1 and run nightly, but run them yourself when you touch codegen,
the build cache, or the generated API:

```bash
make test-ignored  # minutes, not seconds
```

Before #390 nothing in CI ran `cargo test` at all, and this section claimed otherwise —
which is a plausible reason it went unnoticed long enough to cost
[#381](https://github.com/hoodiecollin/forgedb/issues/381) and
[#386](https://github.com/hoodiecollin/forgedb/issues/386).

**2. Code review** by maintainers:
- Code quality
- Test coverage
- Documentation
- API design

**3. Address feedback**:
```bash
# Make changes
git add .
git commit -m "address review feedback"
git push origin feature/my-new-feature
```

**4. Merge** when approved.

**Merge commits, always.** Squash and rebase are disabled in the repository settings, so a
merge commit is the only method GitHub will accept. A branch is a unit of work and its
history is worth keeping — a squash throws away the story the commits were split up to tell,
and a rebase erases the fact that the work happened on a branch at all.

The subject line names the branch and what it did, with the issue in parentheses:

```
Merge feat/238-string-n: string(N) fixed-width inline string columns (#238)
```

**Core work goes through a pull request** (#390). It used to merge locally, and a
merge gate can only stop a merge it sees — a PR-only check would have been watching an
empty road. The branch-per-scope rhythm is unchanged; only the mechanism is. Merge with
the GitHub button (merge commit — squash and rebase are disabled), or locally once the
PR's checks are green:

```bash
make cycle-scope PR=<n>          # or ISSUE=<n> — the gate CI runs on PRs into develop
git checkout develop             # or main, per the coupling rule in CLAUDE.md
git merge --no-ff <branch> -m "Merge <branch>: <what it did> (#<issue>)"
git push origin develop
```

`--no-ff` is load-bearing: without it a branch that is merely ahead fast-forwards, and the
branch boundary disappears exactly as if it had been rebased.

### After Merge

- **Close the issue yourself.** GitHub only honours `Closes #N` for PRs targeting the
  *default* branch, so a PR merged into `develop` leaves its issue open no matter what the
  body says — `closingIssuesReferences` comes back empty. Close it with a comment naming the
  PR and the verification you ran.
- Delete your branch, local and remote.
- Update project board (if applicable).

---

## Code Style

### No comments in ForgeDB's own source

**ForgeDB's source carries no comments.** Not doc comments (`///`, `//!`, `#[doc]`, JSDoc),
not inline `//` prose — on any surface: Rust, TypeScript, `Cargo.toml`, the workflows, the
Makefile. This is enforced, not aspirational:

```bash
make comment-check                          # fails if any disallowed comment exists
bun scripts/strip-comments.ts --write       # fix a tree in place
```

Two reasons. A comment drifts the moment the code beside it moves, and unlike code nothing
compiles it or tests it. And it sits in the grep path of the coding agents that work in this
repo, where a stale line reads as authoritative and sends a whole session down a path that
looks correct the entire way.

So when a piece of code needs explaining, rename or restructure it until it doesn't. When
that genuinely isn't enough, the rationale goes somewhere that can't rot silently, in this
order of preference:

1. **A name** — of the function, the variable, the test.
2. **An `assert!` failure message.** A string literal, so it survives, and it is read at the
   exact moment it matters: when the thing it explains has broken.
3. **A commit message**, then **an issue body**, then **a page on the site**.

**All user-facing documentation lives on the site** (`apps/website/content/docs`). Files
under `docs/` and the website MDX are prose *files*, not comments, and are untouched by this
rule.

#### The one exception

Bugfix commentary whose loss would likely reinvite the defect. It lives **with the test that
covers it**, never at the defect site, and takes a form one grep can audit:

```rust
// REGRESSION(#486): this test used to assert the defect it was hiding.
// It required the literal `fsync = "always"` — the scaffold's own default — so the
// step it pinned changed no generated byte and induced nothing.
```

Continuation lines are ordinary `//` lines beneath the marker. Exactly two exist in the whole
tree. Reaching for a third is a signal to check whether a test name and an assert message can
carry the meaning instead — `grep -rn 'REGRESSION(#'` lists every one.

#### What is not a comment

The checker preserves these, and so must you:

- **Directives** — `//export` and `//go:*` (cgo/Go toolchain), `// @ts-*`, `// eslint-*`,
  `// prettier-ignore`, `//# sourceMappingURL`, `/// <reference`, `#!` shebangs, and the C
  preprocessor's `#include` / `#define` / `#ifndef`.
- **Generated-file markers** — `// Code generated by ForgeDB. DO NOT EDIT.` Go tooling
  matches that line by regex to skip generated files; deleting it changes what `vet`,
  linters and diff tools do.
- **The cgo preamble** before `import "C"`. Its contents are `#cgo` and `#include`
  directives that cgo *compiles*. It is C source that happens to sit inside comment
  delimiters.

This applies to generated output too: `forgedb generate` emits the markers above and nothing
else.

### Rust Style Guide

Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) and use `rustfmt`:

```bash
# Format all code
cargo fmt --all

# Check formatting without changing
cargo fmt --all -- --check
```

### Code Conventions

**Naming** — types `PascalCase`, functions and modules `snake_case`, constants
`SCREAMING_SNAKE_CASE`:

```rust
struct UserTable {}
enum FieldType {}

fn parse_schema() {}
fn validate_field() {}

const MAX_FIELD_LENGTH: usize = 255;

mod parser;
mod validation;
```

**Imports:**
```rust
// Standard library
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// External crates
use serde::{Serialize, Deserialize};
use uuid::Uuid;

// Internal crates
use forgedb_parser::Schema;
use forgedb_storage::Database;

// Local modules
use crate::error::ParseError;
use super::types::FieldType;
```

**Error Handling:**
```rust
// Use Result for recoverable errors
pub fn parse_schema(input: &str) -> Result<Schema, ParseError> {
    // Implementation
}

// Use thiserror for error definitions
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),
    
    #[error("Invalid type: {0}")]
    InvalidType(String),
}

// Use anyhow for application code
use anyhow::{Context, Result};

fn load_schema(path: &Path) -> Result<Schema> {
    let content = fs::read_to_string(path)
        .context("Failed to read schema file")?;
    
    parse_schema(&content)
        .context("Failed to parse schema")
}
```

**Documentation:**
```rust
/// Parse a ForgeDB schema from a string.
///
/// This function tokenizes and parses the input, producing an
/// Abstract Syntax Tree (AST) representing the schema structure.
///
/// # Arguments
///
/// * `input` - Schema definition as a string
///
/// # Returns
///
/// * `Ok(Schema)` - Successfully parsed schema
/// * `Err(ParseError)` - Parse error with details
///
/// # Examples
///
/// ```
/// use forgedb_parser::parse_schema;
///
/// let schema = parse_schema(r#"
///     User {
///       id: +uuid
///       email: string
///     }
/// "#)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// Returns `ParseError` if:
/// - Syntax is invalid
/// - Unknown types are referenced
/// - Directives are malformed
pub fn parse_schema(input: &str) -> Result<Schema, ParseError> {
    // Implementation
}
```

**Comments:**
```rust
// Single-line comment for code explanations

/// Doc comment for public items
/// (use /// for functions, structs, etc.)

//! Module-level doc comment
//! (use //! at the top of modules)
```

### Clippy Lints

Enable strict linting:

```rust
// In lib.rs or main.rs
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]  // Allow if appropriate
```

Run clippy:
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Performance Guidelines

**Avoid unnecessary allocations:**
```rust
// ❌ Bad
fn process(data: String) -> String {
    data.to_uppercase()
}

// ✅ Good
fn process(data: &str) -> String {
    data.to_uppercase()
}
```

**Use appropriate data structures:**
```rust
// ❌ Bad: O(n) lookups
let users = Vec<User>;
users.iter().find(|u| u.id == target_id);

// ✅ Good: O(1) lookups
let users = HashMap<Uuid, User>;
users.get(&target_id);
```

**Profile before optimizing:**
```bash
# Use cargo-flamegraph for profiling
cargo install flamegraph
cargo flamegraph --test my_test
```

---

## Documentation Requirements

### Code Documentation

**All public items must be documented:**
```rust
/// Public struct: Always document
pub struct Database { }

/// Public function: Always document
pub fn open_database() -> Result<Database> { }

// Private function: Optional but recommended
fn internal_helper() { }
```

**Documentation sections:**
- Brief description (one line)
- Detailed explanation (optional)
- Arguments (`# Arguments`)
- Return value (`# Returns`)
- Examples (`# Examples`)
- Errors (`# Errors`)
- Panics (`# Panics`)
- Safety (`# Safety` for unsafe code)

### README Updates

Update relevant READMEs when changing:
- Public APIs
- Usage patterns
- Configuration options
- Installation instructions

**README structure:**
```markdown
# Crate Name

Brief description.

## Features

- Feature 1
- Feature 2

## Usage

```rust
// Code example
```

## Documentation

See [docs.rs](https://docs.rs/crate-name)

## License

MIT or Apache-2.0
```

### Architecture Documentation

Update architecture docs when changing:
- System design
- Component interactions
- Data flow
- Design decisions

See [ARCHITECTURE.md](./ARCHITECTURE.md).

### Changelog

Update `CHANGELOG.md` for all user-facing changes:

```markdown
## [Unreleased]

### Added
- New feature description (#PR)

### Changed
- Changed behavior description (#PR)

### Fixed
- Bug fix description (#PR)

### Deprecated
- Deprecated feature (#PR)

### Removed
- Removed feature (#PR)

### Security
- Security fix description (#PR)
```

---

## Community

### Communication Channels

- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: Questions, ideas, showcase

### Getting Help

**Before asking:**
1. Check documentation
2. Search existing issues

**When asking:**
- Provide context and details
- Include code examples
- Share error messages
- Specify your environment

### Recognition

Contributors are recognized in:
- CONTRIBUTORS.md file
- Release notes
- Social media shout-outs

### License

By contributing, you agree that your contributions will be licensed under:
- MIT License OR
- Apache License 2.0

See [LICENSE-MIT](../LICENSE-MIT) and [LICENSE-APACHE](../LICENSE-APACHE).

---

## Additional Resources

- [Architecture Documentation](./ARCHITECTURE.md)
- [Development Guide](./DEVELOPMENT.md)
- [Publishing Process](./PUBLISHING.md)
- [Public Crates Guide](./PUBLIC_CRATES.md)

---

Thank you for contributing to ForgeDB! 🚀
