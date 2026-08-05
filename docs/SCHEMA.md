# ForgeDB `.forge` Schema Language Reference

The complete, parser-verified reference for the `.forge` schema language: every
type, modifier, relation kind, and directive the compiler accepts. New to
ForgeDB? Start with the [Getting Started](./GETTING_STARTED.md) guide, then use
this as the lookup reference. For 18 worked schemas across many domains, see
[`examples/`](../examples/README.md).

> **Verified against the parser.** Every rule below is grounded in
> `crates/parser/src/{ast.rs, lexer.rs, parser/core.rs}` and the validator. Where
> older docs disagree with this file, this file is correct — see
> [§10 Known Invalid Patterns](#10-known-invalid-patterns-parser-rejects).

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
- **Every model must have an identity field** — a field named `id`, or any field
  carrying the `+` auto-generate modifier. This is fatal, not advisory (#248):
  identity is what `create_*` writes, what the row index is keyed on, what
  relations point at, and what three of the five REST routes take as a path
  parameter, so there is almost no generated surface left without one. Convention
  is `id: +uuid`
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
username: &^string @length(3, 50)       // unique, indexed string with a length constraint
age: ?i32 @min(0) @max(120)             // optional int with numeric range constraints
status: string? @default("pending")     // nullable string with a default (semantic-only marker)
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
| `bytes(N)` | `bytes(20)`      | `[u8; 20]`          | Fixed-size **byte** array (not text — see below) |

**Key points:**
- No `text` type — use `string`
- `bytes(N)` is parsed as `FieldType::Bytes(usize)` and requires `(...)` syntax
- **`bytes(N)` is not a string type.** It holds exactly N raw bytes with no UTF-8
  guarantee, no length tracking, and no text semantics, and it serializes as a JSON
  **array of integers** — `"USD"` in a `bytes(3)` goes on the wire as `[85, 83, 68]`.
  Use it for genuinely binary fixed-width data (a git object id, a digest, a
  fixed-width protocol field). For text of any kind — including short fixed-length
  codes like ISO currency or IATA airport codes — use `string`, with `@length` if you
  want the length enforced.
- **`char(N)` is the deprecated spelling of `bytes(N)`** (#233). It still parses and
  produces identical code, but emits a deprecation warning (`forgedb validate` and
  `forgedb generate` both report it and still exit 0), and it is removed at the next
  major version. The name was a false friend: SQL's `CHAR(N)` is fixed-length *text*.
- **`bytes` is a contextual keyword, not a reserved word** — it only means the type in
  type position followed by `(`, so `bytes: i32` remains a perfectly valid field.
- `json` rides the same variable-length column path as `string` (its serialized JSON, always valid UTF-8, is stored via the string column) but is typed `serde_json::Value`. It is **not indexable, filterable, or sortable** (no `^`/`&` index, no REST `?field=` filter/sort, no `find_by_*`) — JSON has no total order the closed-set matcher can key on. `json?` uses the same 1-byte presence tag as `string?`, so `None` and `Some(Value::Null)` round-trip distinctly.
- `decimal` is an **exact** fixed-point number (`rust_decimal::Decimal`) for money/quantity where `f64` would drift. It rides the fixed **16-byte column** path (like `uuid`), encoded via `Decimal::serialize()`/`deserialize()`. It serializes to/from JSON as a **string** (precision-preserving; the TS SDK types it `string`, OpenAPI `{type:string,format:decimal}`). Because `Decimal` is `Ord`+`Hash` it **is filterable, sortable, and indexable** (`^`/`&`/composite `@index` + `find_by_*`) — the index key is normalized (`.normalize()`) so scale-only differences (`1.0` vs `1.00`) share one bucket. `decimal?` (`Option<Decimal>`) rides the same nullable fixed-byte path as `timestamp?`/`u64?`. Bare `decimal` only — `decimal(p, s)` precision/scale metadata is not yet parsed (deferred).

### Enum types

A closed set of named values is declared as a top-level `enum` (a sibling of `struct`/model) and referenced from a field by its bare PascalCase name (no sigil — sigils `*`/`?`/`[]` are relations):

```
enum OrderStatus { Pending, Paid, Shipped, Delivered, Cancelled }

Order {
  id: +uuid
  status: ^OrderStatus       // indexed enum field
  prev_status: OrderStatus?  // nullable enum -> Option
}
```

- **Declaration:** `enum Name { V1, V2, ... }` — name PascalCase (parser-enforced), variants PascalCase, unique, non-empty; trailing comma optional. Enums may be declared **before or after** the models that reference them.
- **Storage:** a fixed **1-byte `u8` discriminant** column (variants map to `0..N` in declaration order; a codegen error if an enum has more than 256 variants). Nullable enum is a 2-byte `[present, disc]` column, so `None` and `Some(variant-0)` round-trip distinctly.
- **Rust:** a fieldless `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]` enum. **Serialized as the variant NAME string**, so REST / TS / JSON all agree on `"Active"`.
- **TypeScript:** a closed string union (`export type OrderStatus = "Pending" | ...`). **OpenAPI:** `{ "type": "string", "enum": [...] }`.
- **Filter / sort / index:** because the enum is `Ord`+`Hash` it **is filterable, sortable, and indexable** (`^`/`&`/composite `@index` + `find_by_*`) — sort orders by **declaration order** (the discriminant), and the index key is the variant **name** string. No runtime validation is needed on the Rust path (a closed enum cannot hold an invalid variant); an invalid variant string on the REST boundary fails serde `Deserialize` → 4xx automatically.
- A bare PascalCase identifier used as a field type must resolve to a declared enum (or an inline `struct`); otherwise it is an "unknown type" error.

---

## 4. Field Modifiers (Symbols)

### Complete List

| Symbol | Name          | Position | Meaning                                              | Valid On         | Example           |
|--------|---------------|----------|------------------------------------------------------|------------------|-------------------|
| `+`    | Auto-generate | Prefix   | Fill value on insert (`uuid`/`timestamp` only; integer is a marker — see below) | u32, u64, uuid, timestamp | `id: +uuid` |
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
  - **But only `uuid`/`timestamp` are actually synthesized on create today.** `+u32`/`+u64` parse
    and mark the field, but the generator does not fill an integer value — a create must supply
    the integer id. Correct integer auto-increment is RFC #187 (blocked on transaction/coordinator
    sequence-allocation design), deliberately unshipped rather than shipped subtly wrong.
- `&` (unique) can be applied to any field (no type restriction in parser)

**NOT implemented:**
- `~` (auto-update) does not exist — the AST `Field` carries only `auto_generate: bool` (the `+`
  modifier). There is no auto-update-on-write modifier.
- **Integer auto-increment** — `+u32`/`+u64` are parsed and marked but *not synthesized*; only
  `+uuid`/`+timestamp` fill on create. See RFC #187.

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
| `@min`                 | `(number)`      | Numeric (u32/u64/i32/i64/f64) | Minimum value — **ENFORCED** (violation → 422)     | `age: u32 @min(13)` |
| `@max`                 | `(number)`      | Numeric (u32/u64/i32/i64/f64) | Maximum value — **ENFORCED** (violation → 422). *Not* a string-length check — use `@length` for strings. | `age: u32 @max(150)` |
| `@length`              | `(min: n)`, `(max: n)`, `(min: a, max: b)`, `(a, b)`, or `(n)` | `string` | String length in **characters** — **ENFORCED** (violation → 422). See the table below — single-arg `@length(n)` means **exactly** n. | `name: string @length(min: 1, max: 100)` |
| `@email`               | (none)          | `string`            | Email format — **ENFORCED** (violation → 422)      | `email: string @email` |
| `@url`                 | (none)          | `string`            | URL format — **ENFORCED** (violation → 422)        | `website: string @url` |
| `@pattern`             | `(regex_string)` | `string`           | Regex match — **ENFORCED** via `LazyLock<Regex>` (non-match → 422) | `phone: string @pattern("^[0-9]+$")` |
| `@regex`               | `(pattern)`     | `string`            | Regex match — **ENFORCED** (non-match → 422)       | `handle: string @regex("[a-z]+")` |
| `@default`             | `(value)`       | Any                 | Default value on insert — **semantic-only marker** (not applied at write) | `status: string @default("pending")` |
| `@index`               | (none)          | Any                 | Field-level index marker — **semantic-only**; use the `^` modifier to actually index | `slug: string @index` |
| `@computed`            | (none)          | Any                 | Field is computed (read-only) — **semantic-only marker** | `full_name: string @computed` |
| `@fulltext`            | (none)          | `string`            | Full-text search — **semantic-only marker** (no index generated) | `content: string @fulltext` |
| `@materialized`        | (none)          | Any                 | Field is materialized — **semantic-only marker**   | `count: u32 @materialized` |
| `@relations`           | `(*)` or `(field_list)` | Component refs | Component relation inclusion | `card: tsx://path @relations(*)` |
| `@on_delete`           | `(restrict\|cascade\|set_null)` | FK field (`*Target`/`?Target`) | On-delete referential policy (ENFORCED, delete semantics) | `author: *User @on_delete(cascade)` |

**Model-level directives:**

| Directive              | Arguments       | Meaning                                          | Example                      |
|------------------------|-----------------|--------------------------------------------------|------------------------------|
| `@soft_delete`         | (none)          | Enable soft delete                   | `@soft_delete` in model block |
| `@index`               | `(field1, field2, ...)` | Composite index on multiple fields | `@index(user_id, created_at)` |

> **`@length` spellings.** The bound is in **characters** (`chars().count()`), not bytes.
> `min`/`max` may be given in either order, and either alone.
>
> | written | means |
> |---|---|
> | `@length(min: 3)` | at least 3 — a floor with no ceiling |
> | `@length(max: 20)` | at most 20 |
> | `@length(min: 3, max: 64)` | between 3 and 64 |
> | `@length(3, 5)` | between 3 and 5 — the positional pair, unchanged |
> | `@length(5)` | **exactly** 5 |
>
> Mixing positional and named arguments, repeating a name, using a name other than
> `min`/`max`, or writing a `min` above the `max` are all parse errors.
>
> **Single-arg `@length(n)` changed meaning.** It previously meant *at most* n; it now
> means *exactly* n, and the parser emits a warning saying so. Write `@length(max: n)`
> to keep the old behavior. This is a 0.x breaking change: the old spelling still
> parses and still compiles, so a field that accepted shorter values starts returning
> 422 — the warning is the only signal.

> **Quoted string literals.** Directive arguments accept **quoted string literals** in addition
> to numbers and bare identifiers. `@pattern("^[0-9]+$")`, `@regex("...")`, and `@default("text")`
> parse — the lexer tokenizes `"..."` (escapes `\" \\ \n \t \r`; unterminated/multiline strings are
> a lex error). Values are stored as `ConstraintParam::String`, so `@default(pending)` and
> `@default("pending")` are equivalent. `@pattern`/`@regex` are **enforced** (non-match → 422);
> `@default` remains a **semantic-only marker** (parsed, not enforced).
> Superseded the earlier limitation note that said `"` was an unexpected character.

**Parser source:** `crates/parser/src/parser/core.rs:113-184` (constraint parsing, incl. the `Token::Str` arm), `crates/parser/src/parser/core.rs:381-447` (directive parsing); string-literal lexing in `crates/parser/src/lexer.rs` (`read_string`)

**`@on_delete` (ENFORCED — delete semantics):**
- `@on_delete(restrict | cascade | set_null)` on a relation FK field (`*Target` required / `?Target` optional) declares what happens to children when the parent is deleted. It parses as a generic directive (bare-identifier arg — no special lexer rule) and is **enforced by codegen** in the generated `Database::delete_<parent>` wrapper:
  - **`restrict`** (also the DEFAULT when `@on_delete` is absent): refuse to delete a parent that still has any live child referencing it (→ `ValidationError::ReferencedByChildren`, HTTP 409).
  - **`cascade`**: recursively delete every referencing child (each child's own `@on_delete` rules fire, so multi-level chains work; a pathological FK cycle is bounded by `MAX_CASCADE_DEPTH`).
  - **`set_null`**: null each referencing child's FK — **only valid on an OPTIONAL FK (`?Target`)**; `@on_delete(set_null)` on a required `*Target` is a hard codegen error.
- The REST `DELETE /{id}` route goes through this wrapper (Rust API + REST both get integrity). The direct `db.<model>.delete` storage path skips these checks. M2M links to a cascade-deleted model are also unlinked.

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
  layer_digests: [bytes(32); 5]  // array of 5 SHA-256 digests (raw bytes)
  scores: [u32; 10]              // array of 10 unsigned ints
}
```

Note there is no way to put *text* in a fixed array: `string` is variable-length, and
`bytes(N)` is not a string type. Model a list of strings as a related model.
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

**This rules out text.** A struct cannot hold a `string`, and `bytes(N)` is not a string
type — so there is no way to embed an address, a name, or any other free text in a
struct. Text belongs on the model, or on a related model. Structs are for fixed-width
numeric/binary groupings.

**Example:**
```
struct Dimensions {
  length_mm: u32
  width_mm: u32
  height_mm: u32
}

Product {
  id: +uuid
  name: string              // text lives on the model, not in the struct
  packed: Dimensions?       // optional embedded fixed-size group
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

## 8. Component References

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
  - Example files use them (e.g., `apps/vscode-forgedb/examples/example.forge:42-43`), but they will **fail to parse** in the actual CLI
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

2. **@on_delete(set_null) on a REQUIRED FK** (codegen error)
   ```
   author: *User @on_delete(set_null)   // CODEGEN ERROR: a required FK can't be nulled
   ```
   `@on_delete` itself parses and is enforced (see §6); only `set_null` on a required
   `*Target` is rejected (use `?Target`, or `cascade`/`restrict`).

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
  title: &string @length(1, 200)
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
  text: &string @length(1, 1000)
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

### With Components
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

## 12. Where the parser lives

The rules in this reference are grounded in the parser and validator source:

- **AST:** `crates/parser/src/ast.rs`
- **Lexer (tokens):** `crates/parser/src/lexer.rs`
- **Parser logic:** `crates/parser/src/parser/core.rs`
- **Validation:** `crates/validation/src/lib.rs`
- **Example schemas:** [`examples/`](../examples/README.md) and `apps/vscode-forgedb/examples/example.forge`

---

## Summary for New Schema Authors

**Write schemas using this recipe:**

1. **Define models** (PascalCase names) with **snake_case fields**
2. **Use type modifiers** (`+`, `&`, `^`) **before type**, nullable `?` **after type**
3. **Valid scalar types:** u32, u64, i32, i64, f64, bool, string, json, decimal, uuid, timestamp, bytes(N)
4. **Relations:** `[Model]` (one-to-many), `*Model` (required FK), `?Model` (optional FK)
5. **Constraints are ENFORCED at write (violation → 422):** `@min`/`@max` (numeric only), `@length` (string length in characters; `min:`/`max:` named args, and single-arg `@length(n)` means **exactly** n), `@email`, `@url`, `@pattern`/`@regex`. Still semantic-only markers (parsed, not applied): `@default`, `@computed`, `@fulltext`, `@materialized`, field-level `@index`
6. **Composite indexes:** `@index(field1, field2, ...)` at model level (≥2 fields)
7. **Structs:** Define with `struct Name { ... }` and use in models (fixed-size only)
7b. **Enums:** Define with `enum Name { V1, V2, ... }` (PascalCase variants) and reference by bare name; stored as a 1-byte discriminant, serialized as the variant name string, filterable/sortable/indexable
8. **Components:** `field: tsx://path @relations(*)`
9. **Comments:** Only `//` line comments work; `/* */` blocks will **fail to parse**
10. **DO NOT use:** `~` (auto-update), `text` (use `string`), block comments `/* */`, duplicate names, non-PascalCase models, non-snake_case fields

**Verify with:**
```bash
cargo run -- validate --config forgedb.toml
```

