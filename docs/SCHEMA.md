# ForgeDB `.forge` Schema Language Reference

The complete, parser-verified reference for the `.forge` schema language: every
type, modifier, relation kind, and directive the compiler accepts. New to
ForgeDB? Start with the [Getting Started](./GETTING_STARTED.md) guide, then use
this as the lookup reference. For 18 worked schemas across many domains, see
[`examples/`](../examples/README.md).

> **Verified against the parser.** Every rule below is grounded in
> `crates/parser/src/{ast.rs, lexer.rs, parser/core.rs}` and the validator, with
> source line references. Where older docs disagree with this file, this file is
> correct — see [§10 Known Invalid Patterns](#10-known-invalid-patterns-parser-rejects)
> and [§12 Quick Drift Summary](#12-quick-drift-summary-vs-claudemd-quick-ref).

---

## 1. Model/Entity Syntax

### Basic Form
```
ModelName {
  field: type
  field: type
}
```

**Rules:**
- Model name must be **PascalCase** (validated in `crates/parser/src/parser/core.rs:691-696` via `validate_model_name`)
- Models must contain at least one field (`crates/parser/src/parser/core.rs:768`)
- Field names must be **snake_case** (validated in `crates/parser/src/parser/core.rs:462-466` via `validate_field_name`)
- Model names must be unique within schema (`crates/parser/src/parser/core.rs:814-818`)

### Example
```
User {
  id: +uuid
  email: &string @email
  created_at: +timestamp
}
```

---

## 2. Field Syntax

### Form
```
name: [MODIFIER]type [@directive ...]
```

### Position of Modifiers
Modifiers (`+`, `&`, `^`) appear **between the colon and type name**:
```
field: +type      // auto-generate
field: &type      // unique
field: ^type      // indexed
field: +&^type    // multiple modifiers allowed (any combination)
```

**Nullable postfix** (`?`) can appear **after the type**:
```
field: string?              // nullable string
field: MyStruct?            // optional struct reference
field: ?string              // prefix nullable also works
field: +uuid?               // auto-gen + nullable
```

### Field Declaration Rules
- Field name is **required** and must be **snake_case** (`crates/validation/src/lib.rs:296-309`)
- Type is **required** immediately after `:`
- Modifiers (`+`, `&`, `^`) are **optional** and appear **before the type** (`crates/parser/src/parser/core.rs:471-492`)
- Constraints (`@...`) are **optional** and appear **after the type** (`crates/parser/src/parser/core.rs:534-595`)
- Field names must be **unique within a model** (`crates/parser/src/parser/core.rs:752-758`)

### Example
```
username: +&^string @min(3) @max(50)    // auto-gen, unique, indexed string with constraints
age: ?i32 @min(0)                       // optional int with min constraint
status: string? @default("pending")     // nullable string with default
```

---

## 3. Scalar Types (Complete List)

**Verified from `crates/parser/src/lexer.rs` (Token enum) and `crates/parser/src/ast.rs` (FieldType enum):**

| Type      | Form               | Rust Equivalent     | Notes                            |
|-----------|-------------------|---------------------|----------------------------------|
| `u32`     | `u32`             | `u32`               | Unsigned 32-bit integer          |
| `u64`     | `u64`             | `u64`               | Unsigned 64-bit integer          |
| `i32`     | `i32`             | `i32`               | Signed 32-bit integer            |
| `i64`     | `i64`             | `i64`               | Signed 64-bit integer            |
| `f64`     | `f64`             | `f64`               | Floating-point 64-bit            |
| `bool`    | `bool`            | `bool`              | Boolean (true/false)             |
| `string`  | `string`          | `String`            | Variable-length UTF-8 string     |
| `json`    | `json`            | `serde_json::Value` | Arbitrary JSON value (variable-length column, stored as serialized JSON) |
| `decimal` | `decimal`         | `rust_decimal::Decimal` | Exact fixed-point decimal (money/quantity); fixed 16-byte column, JSON string on the wire |
| `uuid`    | `uuid`            | `uuid::Uuid`        | Universal unique identifier      |
| `timestamp` | `timestamp`     | `i64`               | Unix timestamp (milliseconds)    |
| `char(N)` | `char(8)`         | `[u8; 8]`           | Fixed-size byte array (Sprint 8) |

**Key points:**
- No `text` type in parser (CLAUDE.md mentions it but parser has only `string`)
- `char(N)` is parsed as `FieldType::Char(usize)` and requires `(...)` syntax (`crates/parser/src/parser/core.rs:354-369`)
- `json` rides the same variable-length column path as `string` (its serialized JSON, always valid UTF-8, is stored via the string column) but is typed `serde_json::Value`. It is **not indexable, filterable, or sortable** (no `^`/`&` index, no REST `?field=` filter/sort, no `find_by_*`) — JSON has no total order the closed-set matcher can key on. `json?` uses the same 1-byte presence tag as `string?`, so `None` and `Some(Value::Null)` round-trip distinctly.
- `decimal` is an **exact** fixed-point number (`rust_decimal::Decimal`) for money/quantity where `f64` would drift. It rides the fixed **16-byte column** path (like `uuid`), encoded via `Decimal::serialize()`/`deserialize()`. It serializes to/from JSON as a **string** (precision-preserving; the TS SDK types it `string`, OpenAPI `{type:string,format:decimal}`). Because `Decimal` is `Ord`+`Hash` it **is filterable, sortable, and indexable** (`^`/`&`/composite `@index` + `find_by_*`) — the index key is normalized (`.normalize()`) so scale-only differences (`1.0` vs `1.00`) share one bucket. `decimal?` (`Option<Decimal>`) rides the same nullable fixed-byte path as `timestamp?`/`u64?`. Bare `decimal` only — `decimal(p, s)` precision/scale metadata is not yet parsed (deferred).

---

## 4. Field Modifiers (Symbols)

### Complete List

| Symbol | Name          | Position | Meaning                                              | Valid On         | Example           |
|--------|---------------|----------|------------------------------------------------------|------------------|-------------------|
| `+`    | Auto-generate | Prefix   | Automatically generate value on insert               | u32, u64, uuid, timestamp | `id: +uuid` |
| `&`    | Unique        | Prefix   | Field value must be unique (enforced)                | Any type         | `email: &string` |
| `^`    | Indexed       | Prefix   | Create index on this field for faster queries        | Any type         | `slug: ^string`   |
| `?`    | Nullable      | Postfix OR Prefix | Field is optional (NULL allowed)         | Any type         | `age: i32?` or `?i32` |

**Placement rules** (from `crates/parser/src/parser/core.rs:471-492` and `498-523`):
- `+`, `&`, `^` are **prefix modifiers** parsed **before type name**
- Multiple modifiers can be combined: `field: +&^type`
- `?` can appear **before type** (e.g., `?User` for optional reference) or **after type** (e.g., `string?` for nullable primitive)
- Postfix `?` converts struct types to `OptionalStructType` and primitives to `Nullable` wrapper (`crates/parser/src/parser/core.rs:499-523`)

**Validation:**
- `+` (auto-generate) only valid on auto-generatable types: u32, u64, uuid, timestamp (`crates/parser/src/parser/core.rs:526-531`)
- `&` (unique) can be applied to any field (no type restriction in parser)

**NOT implemented:**
- `~` (auto-update) is mentioned in CLAUDE.md quick-ref but does NOT exist in parser code
- No implementation found in AST (`crates/parser/src/ast.rs:Field`) which only has `auto_generate: bool`

---

## 5. Relations

### Relation Type Syntax

| Syntax      | Type               | Meaning                              | Example            |
|-------------|-------------------|--------------------------------------|--------------------|
| `[Model]`  | `OneToMany`       | Parent has many children             | `posts: [Post]`    |
| `*Model`   | `RequiredReference` | Must reference a record (FK, non-NULL) | `author: *User` |
| `?Model`   | `OptionalReference` | Can optionally reference (FK, NULL) | `editor: ?User`  |

**Bidirectional M2M detection** (from `crates/parser/src/ast.rs:233-342`):
- When both models have `[OtherModel]` fields pointing to each other **without a corresponding FK**, the parser auto-detects a `ManyToMany` relationship
- Example:
  ```
  Post {
    tags: [Tag]
  }
  Tag {
    posts: [Post]  // Detected as M2M if neither is a FK reference
  }
  ```

**FK Scalar Generation** (from `crates/parser/src/ast.rs:382-388`):
- `RequiredReference(Model)` fields generate scalar FK columns: `model_id: uuid` (example: `author: *User` → `author_id`)
- `OptionalReference(Model)` fields generate nullable FK columns: `editor_id: Option<uuid>`
- OneToMany and ManyToMany fields are **virtual** (not persisted; stored as empty `()`) (`crates/parser/src/ast.rs:389-391`)

---

## 6. Every `@` Directive (Complete List)

### Directives Parsed by Parser Core

**Field-level directives** (attached to individual fields; parsed as `Constraint` structs):

| Directive              | Arguments       | Field Types         | Meaning                                  | Example                      |
|------------------------|-----------------|---------------------|------------------------------------------|------------------------------|
| `@min`                 | `(number)`      | Numeric (u32, i32, f64, etc.) | Minimum value constraint | `age: u32 @min(13)` |
| `@max`                 | `(number)`      | Numeric + string    | Maximum value or max length              | `age: u32 @max(150)` |
| `@email`               | (none)          | `string`            | Email format validation (semantic)       | `email: string @email` |
| `@pattern`             | `(regex_string)` | `string`            | Regex pattern matching (semantic)        | `phone: string @pattern("^[0-9]+$")` |
| `@length`              | `(min, max)` or `(count)` | `string` | String length constraint (semantic) | `name: string @length(1, 100)` |
| `@default`             | `(value)`       | Any                 | Default value on insert (semantic)       | `status: string @default("pending")` |
| `@url`                 | (none)          | `string`            | URL format validation (semantic)         | `website: string @url` |
| `@regex`               | `(pattern)`     | `string`            | Regex validation (semantic)              | `handle: string @regex("[a-z]+")` |
| `@index`               | (none)          | Any                 | Create field-level index (semantic)      | `slug: string @index` |
| `@computed`            | (none)          | Any                 | Field is computed (read-only; Sprint 19) | `full_name: string @computed` |
| `@fulltext`            | (none)          | `string`            | Full-text search index (Sprint 18)       | `content: string @fulltext` |
| `@materialized`        | (none)          | Any                 | Field is materialized (Sprint 19)        | `count: u32 @materialized` |
| `@relations`           | `(*)` or `(field_list)` | Component refs | Component relation inclusion (Sprint 17) | `card: tsx://path @relations(*)` |

**Model-level directives:**

| Directive              | Arguments       | Meaning                                          | Example                      |
|------------------------|-----------------|--------------------------------------------------|------------------------------|
| `@soft_delete`         | (none)          | Enable soft delete (Sprint 19)                   | `@soft_delete` in model block |
| `@index`               | `(field1, field2, ...)` | Composite index on multiple fields | `@index(user_id, created_at)` |

> **UPDATE (issue #46, 2026-07-06):** directive arguments now accept **quoted string
> literals** in addition to numbers and bare identifiers. `@pattern("^[0-9]+$")`,
> `@regex("...")`, and `@default("text")` parse — the lexer tokenizes `"..."` (escapes
> `\" \\ \n \t \r`; unterminated/multiline strings are a lex error). Values are still stored
> as `ConstraintParam::String`, so `@default(pending)` and `@default("pending")` are
> equivalent; these directives remain **semantic-only markers** (parsed, not enforced).
> Superseded the earlier limitation note that said `"` was an unexpected character.

**Parser source:** `crates/parser/src/parser/core.rs:113-184` (constraint parsing, incl. the `Token::Str` arm), `crates/parser/src/parser/core.rs:381-447` (directive parsing); string-literal lexing in `crates/parser/src/lexer.rs` (`read_string`)

**Directives NOT in Parser (LSP/Semantic only):**
- `@on_delete(cascade|set_null|restrict)` — Found in LSP hover/completion (`crates/lsp-server/src/hover.rs:130`, `crates/lsp-server/src/completion.rs:268-274`) but **NOT parsed by core parser**. Example files use it, but it will not parse.

**Semantic vs. Enforcement:**
- Directives marked "(semantic)" are parsed but **not enforced by the parser**; enforcement is left to validators/codegen
- Example: `@email` is parsed but the parser doesn't validate email format; that's done elsewhere

---

## 7. Composite & Collection Constructs

### Fixed-Size Arrays

**Syntax:**
```
field: [type; count]
```

**Rules:**
- Inner type can be primitive (`u32`, `string`, etc.) or struct name
- Count must be numeric literal
- Parsed as `FieldType::FixedArray(Box<FieldType>, usize)` (`crates/parser/src/ast.rs:55`)
- **Must be fixed-size types** (can't use in variable-length types like `string` inside arrays in structs) (`crates/parser/src/ast.rs:169-176`)

**Example:**
```
Product {
  image_urls: [char(255); 5]    // array of 5 strings (max 255 chars each)
  scores: [u32; 10]              // array of 10 unsigned ints
}
```

### Inline Structs

**Definition syntax:**
```
struct StructName {
  field: type
  field: type
}
```

**Usage in models:**
```
field: StructName          // required struct field
field: StructName?         // optional struct field
```

**Rules:**
- Struct names must be **PascalCase** (same as models)
- Structs can **only contain fixed-size types** (`crates/parser/src/ast.rs:169-176`)
- Struct references in fields are stored as `FieldType::StructType(name)` or `FieldType::OptionalStructType(name)` (`crates/parser/src/ast.rs:56-57`)
- Cannot contain variable-length types (string, relations, components) (`crates/parser/src/ast.rs:169-176`)

**Example:**
```
struct Address {
  street: char(100)
  city: char(50)
  zip: char(10)
}

User {
  id: +uuid
  address: Address?         // optional embedded Address
}
```

### Composite Indexes

**Syntax (model-level):**
```
ModelName {
  field1: type
  field2: type
  @index(field1, field2)
}
```

**Rules:**
- Must include **at least 2 fields** (`crates/parser/src/parser/core.rs:438-440`)
- Fields must exist in the model (`crates/parser/src/parser/core.rs:773-782`)
- Parsed as `CompositeIndex { fields: Vec<String> }` and stored in `Model.composite_indexes` (`crates/parser/src/ast.rs:127`)

**Example:**
```
Order {
  user_id: uuid
  created_at: timestamp
  @index(user_id, created_at)
}
```

### Soft Delete

**Syntax (model-level):**
```
ModelName {
  field: type
  @soft_delete
}
```

**Rules:**
- Model-level directive (not field-level)
- Sets `Model.soft_delete: bool` to `true` (`crates/parser/src/ast.rs:128`)
- Parsed in `crates/parser/src/parser/core.rs:735-738`

**Example:**
```
User {
  id: +uuid
  email: string
  @soft_delete
}
```

---

## 8. Component References (Sprint 17)

### Syntax

```
field: protocol://path [@relations(...)]
```

**Protocols:**
- `tsx://` — TSX (TypeScript React) component
- `jsx://` — JSX component
- `api://` — API route handler

**Path syntax:**
- Path is a series of identifiers separated by `/`
- Examples: `components/user/Card`, `pages/user/profile`, `routes/user/update`

**@relations modifier:**
```
@relations(*)                          // Include all relation fields
@relations(field1, field2, ...)        // Include specific relations
```

**Rules:**
- `ComponentProtocol` enum: `Tsx`, `Jsx`, `Api` (`crates/parser/src/ast.rs:67-72`)
- `ComponentReference` struct stores protocol, path, and relation inclusion (`crates/parser/src/ast.rs:84-88`)
- `@relations` is **only valid on component fields** (`crates/parser/src/parser/core.rs:576-580`)
- Parsed in `crates/parser/src/parser/core.rs:299-330`

**Examples:**
```
User {
  id: +uuid
  posts: [Post]
  comments: [Comment]
  
  profileCard: tsx://components/user/ProfileCard @relations(*)
  avatar: jsx://components/user/Avatar @relations(posts)
  updateEndpoint: api://routes/user/update
}
```

---

## 9. Comments and Whitespace

### Supported Comments

**Line comments:**
```
// This is a comment
field: string  // inline comment
```

**Parsed as:** `Token::Slash` followed by another `Token::Slash`, then skips to end of line (`crates/parser/src/lexer.rs:160-172`)

**NOT supported:**
- Block comments (`/* ... */`) — **NOT parsed by lexer** (`crates/parser/src/lexer.rs` has no `/*` handling)
  - Example files use them (e.g., `vscode-forgedb/examples/example.forge:42-43`), but they will **fail to parse** in the actual CLI
  - This is a **drift issue** — example.forge uses `/* */` but parser doesn't support it

### Whitespace Rules

- **Newlines are significant** — parsed as `Token::Newline` and used to delimit logical tokens
- **Horizontal whitespace** (space, tab) is skipped via `skip_whitespace()` (`crates/parser/src/lexer.rs:93-101`)
- **Carriage returns** (`\r`) are also skipped (`crates/parser/src/lexer.rs:94-95`)
- Model/struct definitions can span multiple lines (newlines are skipped between major tokens)

**Terminators:**
- No semicolons required for fields or models (only `{` and `}` block delimiters)
- `@` directives must appear after field type (before newline or next constraint)

---

## 10. Known Invalid Patterns (Parser Rejects)

### Cannot Parse

1. **Block comments**
   ```
   /* This will fail */
   User { id: +uuid }
   ```
   Parser error: unexpected `/` and `*` tokens

2. **@on_delete directive** (not in parser, only in LSP)
   ```
   author: *User @on_delete(cascade)   // PARSE FAILS in forgedb validate
   ```
   Parser accepts it as a generic constraint but LSP validates semantics separately

3. **Duplicate field names**
   ```
   User {
     id: +uuid
     id: string    // ERROR: duplicate field
   }
   ```

4. **Duplicate model/struct names**
   ```
   User { id: +uuid }
   User { email: string }   // ERROR: duplicate model
   ```

5. **Model/struct without fields**
   ```
   User { }   // ERROR: model has no fields
   ```

6. **Wrong auto-generate type**
   ```
   count: string +   // ERROR: only u32, u64, uuid, timestamp support +
   ```

7. **Nullable primitive inline without wrapping in parent**
   ```
   age: ?u32       // OK
   age: u32?       // OK
   age: ??u32      // Double nullable — probably invalid, untested
   ```

8. **Non-PascalCase model/struct names**
   ```
   user { id: +uuid }   // ERROR: must be 'User' (PascalCase)
   ```

9. **Non-snake_case field names**
   ```
   User {
     userId: +uuid   // ERROR: must be 'user_id' (snake_case)
   }
   ```

10. **Struct containing variable-length types**
    ```
    struct Address {
      street: string    // ERROR: string is variable-length
    }
    ```

11. **Composite index with < 2 fields**
    ```
    Order {
      id: +uuid
      @index(id)    // ERROR: need at least 2 fields
    }
    ```

12. **Composite index referencing non-existent field**
    ```
    Order {
      id: +uuid
      @index(id, missing_field)   // ERROR: field not found
    }
    ```

13. **Component field without protocol**
    ```
    card: path/to/component     // ERROR: need tsx://, jsx://, or api://
    ```

14. **@relations on non-component field**
    ```
    id: +uuid @relations(posts)   // ERROR: only component fields
    ```

15. **Relation to undefined model**
    ```
    author: *Undefined    // ERROR: model 'Undefined' doesn't exist
    ```

---

## 11. Example Valid Schemas

### Minimal Valid Schema
```
User {
  id: +uuid
  email: &string
}
```

### With Modifiers and Constraints
```
Post {
  id: +uuid
  title: &string @max(200)
  slug: ^&string @length(1, 100)
  content: string
  view_count: u32 @default(0)
  published: bool @default(false)
  published_at: timestamp?
  created_at: +timestamp
  author: *User
  comments: [Comment]
}

Comment {
  id: +uuid
  text: &string @max(1000)
  author: *User
  post: *Post
  created_at: +timestamp
}

User {
  id: +uuid
  email: ^&string @email
  posts: [Post]
  comments: [Comment]
}
```

### With Structs
```
struct GeoLocation {
  latitude: f64
  longitude: f64
}

Venue {
  id: +uuid
  name: &string
  location: GeoLocation
  created_at: +timestamp
}
```

### With Components (Sprint 17)
```
User {
  id: +uuid
  email: string
  posts: [Post]
  
  profileCard: tsx://components/user/ProfileCard @relations(posts)
  updateEndpoint: api://routes/user/update
}
```

### With Composite Indexes
```
Order {
  id: +uuid
  user_id: uuid
  status: string
  created_at: timestamp
  
  @index(user_id, created_at)
  @index(status, created_at)
}
```

---

## 12. Quick Drift Summary (vs. CLAUDE.md Quick-Ref)

### What CLAUDE.md Claims vs. Parser Reality

| CLAUDE.md Claim | Parser Reality | Status |
|---|---|---|
| `~` auto-update modifier | Not in AST or parser | **DRIFT** — `~` not implemented |
| `text` type | Not in lexer (only `string`) | **DRIFT** — no `text` type |
| `@unique` directive | `&` symbol is used, not `@unique` directive | **PARTIALLY CORRECT** — `&` is the modifier; `@unique` would be constraint |
| `@pattern` constraint | Parsed as generic constraint, not validated | **CORRECT** — parsed but semantic validation deferred |
| `@relations(...)` | Only valid on component fields | **CORRECT** |
| `[Model]` one-to-many | Correct | **CORRECT** |
| `*Model` required FK | Correct | **CORRECT** |
| `?Model` optional FK | Correct | **CORRECT** |
| Block comment `/* */` | **NOT supported** by lexer | **DRIFT** — example.forge uses it but won't parse |
| All listed directives | Several not in parser (e.g., `@on_delete`) | **PARTIAL DRIFT** — example uses `@on_delete` but parser doesn't parse it |

---

## 13. File References (Citation Index)

- **AST ground truth:** `/Users/collin/Projects/forgedb/crates/parser/src/ast.rs`
  - FieldType enum: lines 43-64
  - Field struct: lines 103-114
  - Model struct: lines 124-129
  - Struct struct: lines 117-121
  - Constraint struct: lines 12-29

- **Lexer (tokens):** `/Users/collin/Projects/forgedb/crates/parser/src/lexer.rs`
  - Token enum: lines 13-54
  - Comment handling: lines 110-118
  - Type tokenization: lines 240-254

- **Parser logic:** `/Users/collin/Projects/forgedb/crates/parser/src/parser/core.rs`
  - Field parsing: lines 445-612
  - Model parsing: lines 675-790
  - Struct parsing: lines 614-673
  - Type parsing: lines 183-341
  - Constraint parsing: lines 113-181
  - Composite index parsing: lines 377-443
  - Component reference parsing: lines 63-98, 299-330
  - Modifier validation: lines 471-492, 498-523

- **Validation:** `/Users/collin/Projects/forgedb/crates/validation/src/lib.rs`
  - Field name validation: lines 296-309
  - Model name validation: lines 311-324

- **Example schemas:**
  - `/Users/collin/Projects/forgedb/schema.forge`
  - `/Users/collin/Projects/forgedb/vscode-forgedb/examples/example.forge`
  - `/Users/collin/Projects/forgedb/examples/component-integration/schema.forge`

- **Templates:** `/Users/collin/Projects/forgedb/src/templates.rs` (lines 2-112)

- **Quick reference:** `/Users/collin/Projects/forgedb/CLAUDE.md` (lines 98-104)

---

## Summary for New Schema Authors

**Write schemas using this recipe:**

1. **Define models** (PascalCase names) with **snake_case fields**
2. **Use type modifiers** (`+`, `&`, `^`) **before type**, nullable `?` **after type**
3. **Valid scalar types:** u32, u64, i32, i64, f64, bool, string, json, decimal, uuid, timestamp, char(N)
4. **Relations:** `[Model]` (one-to-many), `*Model` (required FK), `?Model` (optional FK)
5. **Constraints** (`@min`, `@max`, `@email`, `@length`, `@default`, `@regex`, `@url`) are parsed but semantic validation is deferred
6. **Composite indexes:** `@index(field1, field2, ...)` at model level (≥2 fields)
7. **Structs:** Define with `struct Name { ... }` and use in models (fixed-size only)
8. **Components:** `field: tsx://path @relations(*)` (Sprint 17)
9. **Comments:** Only `//` line comments work; `/* */` blocks will **fail to parse**
10. **DO NOT use:** `~`, `text`, block comments, `@on_delete`, duplicate names, non-PascalCase models, non-snake_case fields

**Verify with:**
```bash
cargo run -- validate --config forgedb.toml
```

