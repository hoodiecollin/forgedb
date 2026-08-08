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
| `string(N)` | `string(32)`    | `String` (`InlineStr<N>` as a key) | **At most** N characters, in a fixed-width column slot (see below) |
| `string(N!)` | `string(3!)`   | `String` (`InlineStr<N>` as a key) | **Exactly** N characters, in a fixed-width column slot |
| `json`    | `json`            | `serde_json::Value` | Arbitrary JSON value (variable-length column, stored as serialized JSON) |
| `decimal` | `decimal`         | `rust_decimal::Decimal` | Exact fixed-point decimal (money/quantity); fixed 16-byte column, JSON string on the wire |
| `uuid`    | `uuid`            | `uuid::Uuid`        | Universal unique identifier      |
| `timestamp` | `timestamp`, `timestamp(s\|ms\|us)` | `forgedb_types::Timestamp` | An instant. Stored as `i64` **microseconds** since the Unix epoch; RFC 3339 string on the wire (see below) |
| `bytes(N)` | `bytes(20)`      | `[u8; 20]`          | Fixed-size **byte** array (not text — see below) |

**Key points:**
- No `text` type — use `string`
- `bytes(N)` is parsed as `FieldType::Bytes(usize)` and requires `(...)` syntax
- **`bytes(N)` is not a string type.** It holds exactly N raw bytes with no UTF-8
  guarantee, no length tracking, and no text semantics, and it serializes as a JSON
  **array of integers** — `"USD"` in a `bytes(3)` goes on the wire as `[85, 83, 68]`.
  Use it for genuinely binary fixed-width data (a git object id, a digest, a
  fixed-width protocol field). For text of any kind — including short fixed-length
  codes like ISO currency or IATA airport codes — use `string(N!)`, which is text on
  the wire, length-checked on every write, and stored in a fixed slot.
- **`char(N)` is the deprecated spelling of `bytes(N)`** (#233). It still parses and
  produces identical code, but emits a deprecation warning (`forgedb validate` and
  `forgedb generate` both report it and still exit 0), and it is removed at the next
  major version. The name was a false friend: SQL's `CHAR(N)` is fixed-length *text*.
- **`bytes` is a contextual keyword, not a reserved word** — it only means the type in
  type position followed by `(`, so `bytes: i32` remains a perfectly valid field.
- `json` rides the same variable-length column path as `string` (its serialized JSON, always valid UTF-8, is stored via the string column) but is typed `serde_json::Value`. It is **not indexable, filterable, or sortable** (no `^`/`&` index, no REST `?field=` filter/sort, no `find_by_*`) — JSON has no total order the closed-set matcher can key on. `json?` uses the same 1-byte presence tag as `string?`, so `None` and `Some(Value::Null)` round-trip distinctly.
- `f64` **is filterable, sortable, and indexable** (`^`/`&`/composite `@index` + `find_by_*` + `find_by_*_range`), even though Rust's `f64` has no `Ord`. The index key is the IEEE 754 **total-order encoding**, which gives a strict order `-Inf < negatives < ±0 < positives < +Inf < NaN` — so `NaN` and both infinities are each their own bucket rather than sharing the null bucket with an unset value (#242). Two canonicalizations follow from float equality: `-0.0` keys as `0.0` (they compare equal, so a probe of one finds the other), and all `NaN` payloads fold to one key. Note `NaN` sorts **above** every number, so it is excluded from any finite range. For money or quantities use `decimal` — binary floats cannot represent `0.1` exactly, and no index key can repair that.
- `decimal` is an **exact** fixed-point number (`rust_decimal::Decimal`) for money/quantity where `f64` would drift. It rides the fixed **16-byte column** path (like `uuid`), encoded via `Decimal::serialize()`/`deserialize()`. It serializes to/from JSON as a **string** (precision-preserving; the TS SDK types it `string`, OpenAPI `{type:string,format:decimal}`). Because `Decimal` is `Ord`+`Hash` it **is filterable, sortable, and indexable** (`^`/`&`/composite `@index` + `find_by_*`) — the index key is normalized (`.normalize()`) so scale-only differences (`1.0` vs `1.00`) share one bucket. `decimal?` (`Option<Decimal>`) rides the same nullable fixed-byte path as `timestamp?`/`u64?`. Bare `decimal` only — `decimal(p, s)` precision/scale metadata is not yet parsed (deferred).

### Timestamps and declared precision — `timestamp(s|ms|us)`

A `timestamp` is an **instant**, not a wall-clock time and not a date: there is no
timezone, and `Z` is the only offset ever emitted.

```
Trade {
  id:         +timestamp(us)   // an allocated key, microsecond-precise
  filled_at:  timestamp(ms)
  settled_on: timestamp(s)
  created_at: +timestamp       // a stamp — bare, so milliseconds
}
```

**Storage is always microseconds.** The declared key does not change what is on
disk; it is the **quantum**:

- a value you supply is **floored** to it on write (floor, never truncate — a
  pre-epoch value truncated toward zero would round *forward* in time);
- an allocated `+timestamp` identity advances by one unit of it.

So precision buys *fidelity*, not correctness. `timestamp(s)` is a promise that the
stored value is second-aligned, which is what makes it meaningful to display, index
and compare as a second.

A bare `timestamp` is **`timestamp(ms)`**. Milliseconds is the default because it
matches what almost every producer of a timestamp actually has, and because
seconds — the pre-v0.4.0 unit — cannot order two rows written in the same second.

`ns` is not offerable: microseconds is the storage unit, and `i64` nanoseconds would
cap the type at 1678–2262.

**The wire form is RFC 3339**, with six fractional digits and a `Z`:

```json
{ "created_at": "2026-03-31T23:33:20.123456Z" }
```

That is the form in JSON bodies, the TS SDK (`string`), the OpenAPI document
(`{"type":"string","format":"date-time"}`), the Rust/Python/Go REST SDKs, and the
REST filter parameters — every surface that goes through serde. It is *not* the form
of the **index key**, which stays the stored number so a timestamp index keeps
numeric order rather than lexicographic order.

Two consequences worth knowing:

- **An instant outside RFC 3339's year range (`0000`–`9999`) is rejected with a
  422.** `i64` microseconds reaches ±292 000 years, so such a value is *storable* and
  not *serializable* — a row that could be written and would then fail on every read.
  The write path refuses it instead.
- **A timestamp key survives a URL path segment** by construction: RFC 3339 contains
  no reserved URL character. This is what makes `id: +timestamp(us)` usable as a REST
  identity.

**`+timestamp` as an identity.** An auto-generate timestamp is a legal primary key,
with two rules:

1. **It must be named `id`.** Under any other name a `+timestamp` is a *stamp* —
   `created_at`, `seen_at` — and inferring a primary key from one would silently
   mis-key the model. (This is deliberately asymmetric with `+u32`/`+u64`: the only
   reason to write an auto integer is to get an allocated sequence, so an auto
   integer is unambiguously key-ish.)
2. **It must be declared `us`.** `id: +timestamp` and `id: +timestamp(ms)` are
   rejected, naming the floor.

The reason for rule 2 is that **precision does not make a key unique — monotonic
allocation does.** The allocator is `next = max(now, last + 1)`, so a burst of
inserts inside one clock tick still yields distinct, strictly increasing keys, but it
does so by running the counter *ahead* of the wall clock. Recovery time is
proportional to the declared unit: a million-row import lands rows about 17 minutes in
the future at `ms`, and one second ahead at `us`.

`0` (1970-01-01T00:00:00Z) is the "not set" sentinel for a `+timestamp`, so it cannot
be inserted explicitly — supplying it means "generate one". Supplying any other value
is honoured verbatim *and* advances the counter past it.

### Inline fixed-width strings — `string(N)` and `string(N!)`

A bare `string` lives in the variable-length column: the row stores a 16-byte
`(offset, length)` pair and the bytes live elsewhere, so reading one costs a second
lookup and a copy. `string(N)` instead reserves a **fixed slot in the row itself**, so
the value is read by slicing the row's own bytes.

```
enum Level { Silver, Gold }

Account {
  id:       +uuid
  code:     &string(12)      // at most 12 characters, unique
  currency: ^string(3!)      // exactly 3 — an ISO 4217 code
  label:    string(24)?      // nullable
  tier:     Level
  bio:      string           // still variable-length; nothing changed here
}
```

- **N counts CHARACTERS, not bytes** — the same unit `@length` uses.
- **`string(N)` is a maximum; `string(N!)` is an exact length.** Both are **ENFORCED**
  on every insert and update (violation → 422), like `@pattern`, not only at
  validation time. `N` must be between 1 and 255.
- **The value is ASCII by default.** One byte per character is what makes the slot
  small enough to win, so a non-ASCII character is rejected (422) unless the field
  opts in with `@utf8` — which widens the reservation to four bytes per character and
  otherwise changes nothing about the type. `@utf8` on a bare `string` (or on a
  non-string) is a schema error, because there is nothing there to widen.
- **On the wire it is a `string`.** Rust `String`, TypeScript `string`, JSON string,
  OpenAPI `{"type": "string", "maxLength": N}` (plus `minLength` for the exact form).
  A client cannot tell the two spellings apart; only the storage differs.
- **Filterable, sortable, indexable** (`^` / `&` / composite `@index` / `find_by_*`) —
  it is a string everywhere the closed-set matcher is concerned.
- **The length directives do not apply.** The width in the type is already the bound,
  so `@max`, `@length(max: n)`, `@length(n)` and the max component of `@length(a, b)`
  are schema errors on `string(N)` — declare the width you mean instead. `@min` and
  `@length(min: n)` still work, because a lower bound is something the type does not
  say. On `string(N!)` the length is fully determined, so **every** length directive
  is an error. A bare `string` keeps all of them.
- **Above 64 characters the parser warns and still generates.** Experiment #261
  measured a fixed slot against pointer storage across 200 configurations: the slot
  wins while it is small and loses once it is wide. Past that point prefer `string`
  unless the value has to be a fixed-width key.
- **It cannot go inside a `struct` or a `[T; N]`.** Those store their fields as the
  Rust value's bytes, and the Rust value here is a heap `String` — see §7. Use
  `bytes(N)` there.
- **It can be a model's identity** — see the next section, which is where the Rust
  value stops being a `String`.

### `string(N)` as an identity

Both spellings are legal identities (#252):

```
Airport {
  id:   string(3!)          // exactly 3 — an IATA code
  city: string
}

Isbn {
  id:    string(17)         // at most 17
  title: string
}
```

- **A key is not a `String`.** In a key position the Rust type is
  `forgedb_types::InlineStr<N>` — a `Copy`, fixed-capacity string, so it can sit in
  the row index, in a junction `HashMap`, and in a fixed-width replication frame the
  way every other key type does. On every wire it is still a plain string: JSON
  string, TypeScript `string`, OpenAPI `{"type": "string"}`. A client cannot tell.
  An ordinary `string(N)` *column* is unaffected and stays a `String`.
- **A bare `string` identity is refused.** A key has to be fixed-width to be `Copy`,
  so the width has to be in the type. The error tells you to write one.
- **`@utf8` on an identity is a schema error.** The alphabet below is a strict subset
  of ASCII, so `@utf8` would reserve four bytes per character to hold characters the
  write path rejects.
- **A key's value must survive a URL path segment, byte for byte.** Enforced on every
  insert and update (violation → 422): the legal characters are RFC 3986 `pchar`
  **minus** `%` — `A-Z a-z 0-9 - . _ ~ ! $ & ' ( ) * + , ; = : @`. `%` is excluded so
  that no escaping is involved at all and `GET /airports/SFO` is the row's address
  literally. Note that `:` and `@` are `pchar` without being *unreserved*, which is
  what admits `urn:isbn:0451450523` and `user@example.com` as keys.
- **The empty string is rejected**, since `/airports/` addresses the collection rather
  than a row.
- **A string-keyed model is an ordinary relation target.** `*Model` / `?Model` FKs,
  `@on_delete` in all three modes, forward traversal, reverse getters, eager load and
  many-to-many junctions all work exactly as they do for a uuid-keyed model — an FK's
  column simply follows the target's key and is N bytes wide.
- **The width is a per-row cost paid twice.** The identity map is
  `HashMap<InlineStr<N>, usize>`, held in RAM, and every FK pointing at the model
  carries the same N bytes on every child row. The 64-character advisory above is a
  *scan-bandwidth* threshold and does not cover this; a wide key is worth thinking
  about at a narrower width than a wide column is.

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
| `+`    | Auto-generate | Prefix   | Fill value on insert when omitted (all four types)   | u32, u64, uuid, timestamp | `id: +uuid` |
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
  - **All four synthesize on create** (#187). Each type has an "unset" sentinel the create path
    looks for: a nil UUID, a zero timestamp, and — for `+u32`/`+u64` — **`0`**.
  - **A `+timestamp` is a stamp by default and a key only when named `id`** (#254). Under any
    other name it is filled with `now()`, floored to the field's declared precision; named `id`
    it becomes an allocated identity, and must then be declared `us`
    (`id: +timestamp(us)`). See *Timestamps and declared precision* above.
  - **`0` cannot be inserted explicitly** into an auto-integer field. Supplying it means
    "allocate one for me". Supplying any other value is honoured verbatim *and* advances the
    counter past it, so restoring a backup or importing a dataset does not collide on the next
    insert.
  - **Integer autos are monotonic and unique, not contiguous.** A rolled-back transaction, or an
    attempt the commit coordinator rejects, burns its number — the same contract Postgres and
    MySQL offer. Do not rely on the sequence being gapless.
  - **Any integer-auto shape is valid** — the identity, `&unique`, `^`, or a bare
    `seq: +u64`. The counter is per-process, so two writers coordinated through
    `forgedb coordinate` can allocate the same number; what makes that *detected* rather than
    silent is the write-set the coordinator compares, and every shape puts a claim there — the
    identity via its row key, `&unique` via its unique claim, and a bare field via its own
    sequence claim (#260). Marking it `&` is still worth considering when you want uniqueness
    *enforced* against all history: that index is durable, whereas a sequence claim is only as
    old as the running coordinator.
- `&` (unique) can be applied to any field (no type restriction in parser)
- **`&`/`^` on the model's identity field warns** (#258). The identity is already
  unique — the generated `id_to_row` is a map keyed by it — so the modifier has no
  effect and no secondary index is built for it. The schema stays **valid**; this is
  advisory only, and the fix is to drop the modifier:

  ```forge
  Widget { id: &+uuid }   // ⚠ '&' has no effect — id is already the primary key
  Widget { id: +uuid }    // ✓
  ```

  The *identity* is what is excluded, not "any `+` field". On a **non-identity**
  auto field, `&` and `^` are fully meaningful and enforced:

  ```forge
  Event {
    id: +uuid
    ref_id: &+uuid        // ✓ indexed AND unique — a duplicate is rejected
    seen_at: ^+timestamp  // ✓ indexed
  }
  ```

  (Before #258 both of those were silently dropped: no index was built and `&`
  enforced nothing. That affected `+uuid`/`+timestamp`, which synthesize today.)

**NOT implemented:**
- `~` (auto-update) does not exist — the AST `Field` carries only `auto_generate: bool` (the `+`
  modifier). There is no auto-update-on-write modifier.
- **Gapless / contiguous sequences.** `+u32`/`+u64` are monotonic and unique only; see the
  `+` notes above.

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

**FK Scalar Generation:**
- A foreign key's type and column width **follow the target model's identity field**. `author: *User`
  where `User { id: +uuid }` is a 16-byte uuid column; where `User { id: +u64 }` it is an 8-byte
  `u64` column. There is no separate FK type — an FK column is physically identical to the column
  the target's identity itself occupies.
- `RequiredReference(Model)` → a scalar column of the target's key type.
- `OptionalReference(Model)` → the nullable form of the same (`Option<K>`).
- OneToMany and ManyToMany fields are **virtual** (not persisted; stored as empty `()`).
- An identity field may itself be a foreign key (`Order { id: *Customer }`), in which case it
  resolves to whatever `Customer`'s identity ultimately is. That chain must **terminate** —
  `Left { id: *Right }` / `Right { id: *Left }` is a validation error naming both ends.
- A wide inherited key is warned about on the *referencing* field: an FK to a model whose identity
  is a wide `string(N)` pays that width on every row of the child.
- **Many-to-many endpoints:** a junction stores each endpoint's id in a fixed-width, hashable
  column, so an endpoint's identity must be `uuid`, an integer type, `timestamp`, or
  `string(N)`/`string(N!)` — the last of which qualifies because a key of that type is a
  `Copy`, fixed-capacity `InlineStr<N>` rather than a heap `String` (#252). Mixed pairs
  are fine — a `+u64`-keyed model links to a `+uuid`-keyed one, and each junction column is that
  endpoint's own width. An identity outside that set is a validation error rather than a silently
  missing M2M surface.

---

## 6. Every `@` Directive (Complete List)

### Directives Parsed by Parser Core

**Field-level directives** (attached to individual fields; parsed as `Constraint` structs):

| Directive              | Arguments       | Field Types         | Meaning                                  | Example                      |
|------------------------|-----------------|---------------------|------------------------------------------|------------------------------|
| `@min`                 | `(n)` or `(>n)` | Numeric (u32/u64/i32/i64/f64/decimal) | Minimum value — **ENFORCED** (violation → 422). `>n` is an exclusive bound (continuous types only). | `age: u32 @min(13)`, `rate: f64 @min(>0)` |
| `@max`                 | `(n)` or `(<n)` | Numeric (u32/u64/i32/i64/f64/decimal) | Maximum value — **ENFORCED** (violation → 422). `<n` is exclusive. *Not* a string-length check — use `@length` for strings. | `age: u32 @max(150)`, `rate: f64 @max(<1)` |
| `@length`              | `(min: n)`, `(max: n)`, `(min: a, max: b)`, `(a, b)`, or `(n)` | `string` | String length in **characters** — **ENFORCED** (violation → 422). See the table below — single-arg `@length(n)` means **exactly** n. | `name: string @length(min: 1, max: 100)` |
| `@utf8`                | (none)          | `string(N)` only, and never on an identity | Widen an inline string's slot to four bytes per character — **ENFORCED** (without it a non-ASCII character is a 422). An error on any other type, and an error on a model's identity field (#252 — a key's alphabet is a strict ASCII subset, so there would be nothing to widen). | `title: string(60) @utf8` |
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

Note there is no way to put *text* in a fixed array. `string` is variable-length;
`string(N)` has a fixed *column* slot but its Rust value is a heap `String`, and a
fixed array is stored by writing the element values' bytes — embedding one would
persist a pointer, so it is a schema error. `bytes(N)` is not a string type. Model a
list of strings as a related model.
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
struct. Nor can it hold a `string(N)`: that type's *column* is fixed-width, but a struct
is persisted by writing the Rust value's bytes and the Rust value is a heap `String`, so
embedding one would store a pointer. Text belongs on the model, or on a related model.
Structs are for fixed-width numeric/binary groupings.

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
3. **Valid scalar types:** u32, u64, i32, i64, f64, bool, string, string(N) / string(N!), json, decimal, uuid, timestamp / timestamp(s|ms|us), bytes(N)
4. **Relations:** `[Model]` (one-to-many), `*Model` (required FK), `?Model` (optional FK)
5. **Constraints are ENFORCED at write (violation → 422):** `@min`/`@max` (numeric only, `decimal` included — each compares in its own domain, so a `decimal` bound stays exact and a 64-bit integer bound never rounds; bounds may be negative or fractional, and `>n`/`<n` make them exclusive on `f64`/`decimal`), `@length` (string length in characters; `min:`/`max:` named args, and single-arg `@length(n)` means **exactly** n), `@email`, `@url`, `@pattern`/`@regex`, `@utf8` (inline strings only). Still semantic-only markers (parsed, not applied): `@default`, `@computed`, `@fulltext`, `@materialized`, field-level `@index`
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

