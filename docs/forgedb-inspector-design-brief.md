# ForgeDB — Project Brief for the Inspector Design

*A context document for a design agent. The goal here is to give you an accurate, complete
mental model of what ForgeDB is, what a "database" produced by it actually contains, and what
a user can **do** with that data — so you can design a database **inspector** freely and
organically. It deliberately does **not** prescribe layouts, components, screens, or visual
style. Where it goes into detail is exactly where the domain forces design decisions: the
**input/value types** the data is made of, and the **power-user controls** the capabilities
afford.*

---

## 1. What ForgeDB is (so you know what you're inspecting)

ForgeDB is an **application-database generator**. A developer writes a declarative schema (a
`.forge` file) describing their app's data — models, fields, types, relations, constraints —
and ForgeDB **generates tailored code** for that specific schema: a Rust database layer, a
TypeScript SDK, a REST/WebSocket API, and UI stubs.

The important consequence for you: **there is no generic database engine at the center.** Each
generated database is bespoke to one app's schema. A `User` model with `email`, `posts`, and a
`created_at` timestamp exists as *generated, hand-tailored-looking code*, not as rows in a
generic table that some universal engine interprets.

**What that means for the inspector you're designing:** the inspector is a **developer/ops
tool** that points at one of these generated databases and lets a human see and manipulate its
data. Think "the window into an app's live data during development and operations" — closer in
spirit to a database GUI (pgAdmin, Prisma Studio, TablePlus, Django admin, MongoDB Compass)
than to an IDE or a BI dashboard. But unlike those, every ForgeDB database has a **known,
strongly-typed, relational shape** declared up front — you always know the exact fields, types,
and relationships. There is no "arbitrary unknown table" case. This is a gift for design: the
UI can be deeply type-aware.

The inspector is a **desktop application**. It is standalone — it inspects a database; it is
never part of the app being inspected.

---

## 2. Where the inspector's data comes from (two sources, deliberately different)

You don't need to internalize the architecture, but two facts shape what's possible on each
screen:

1. **At-rest, structural data** — the inspector can read the database's files directly (row
   counts, storage sizes, dead-row counts, raw column bytes, the schema shape). This works
   **without the app running**. It's cheap, always available, and "physical."

2. **Typed, live data** — to see actual *typed records* (a real `User` with its `email` and its
   `posts`), to run filtered queries, to create/edit/delete data, or to watch live changes, the
   inspector talks to the app's **running generated API server**. This requires the app (or a
   dev instance of it) to be running and attached.

So there's a natural **"connected vs. not-connected"** duality: some capabilities are always
available (structure, stats, schema shape, raw storage), others require an attached running
database. How you surface that split — gracefully degrade, prompt to connect, launch a server,
etc. — is an open design question, not a settled one.

---

## 3. The data model — **this is the input-type surface** (read carefully)

Every ForgeDB database is a set of **models** (entities/tables) and **structs** (embedded
fixed-shape value objects). Each model has named **fields**. A field has a **type**, optional
**modifiers**, and optional **directives** (constraints/hints). Understanding the value domain
of each is what lets you design correct input, display, filter, and edit controls.

### 3a. Scalar field types — value domains and edge cases

| Type        | What it holds                                  | Input / display considerations |
|-------------|------------------------------------------------|--------------------------------|
| `u32`       | Unsigned integer, 0 … ~4.29e9                  | Non-negative only; bounded. Reject negatives/decimals. |
| `u64`       | Unsigned integer, 0 … ~1.8e19                  | Can exceed JS safe-integer range — precision matters if you format/edit as a number vs. string. |
| `i32`       | Signed integer, ~±2.1e9                        | Negative allowed. |
| `i64`       | Signed integer, ~±9.2e18                       | Same big-number precision caveat as `u64`. |
| `f64`       | 64-bit float                                   | Decimals, scientific notation, special values (NaN/Inf) are edge cases. |
| `bool`      | true / false                                   | Two-state. But note nullable `bool?` = three-state (true/false/absent). |
| `string`    | Variable-length UTF-8 text                     | Arbitrary length, multi-line possible, unicode/emoji. The only unbounded-size scalar. |
| `uuid`      | 128-bit UUID                                   | Usually system-generated (see `+` below). Fixed format; often shown truncated. Used as primary keys and foreign keys. |
| `timestamp` | Unix time in **milliseconds** (i64 under hood) | Present to humans as dates/times; the underlying value is a millisecond integer. Timezone display is a design choice. |
| `char(N)`   | **Fixed-size** byte array of exactly N bytes   | Fixed cardinality (e.g. `char(10)` = 10 bytes). Fixed-width codes, hashes, fixed identifiers. Distinct from `string`. |

Key nuances a designer must respect:
- **`u64`/`i64` precision:** these can hold values that lose precision if handled as ordinary
  floating-point numbers. Editing them may need string-based input.
- **`timestamp` is millisecond-precision integer time**, not a formatted date string. Any
  date-picker affordance has to round-trip to ms.
- **`char(N)` is fixed-length and byte-oriented**, unlike the free-form `string`.

### 3b. Field modifiers (change the *behavior* of a field, not its base type)

| Modifier | Meaning                                   | Design implication |
|----------|-------------------------------------------|--------------------|
| `+`      | **Auto-generate** on insert (only on `u32`/`u64`/`uuid`/`timestamp`) | The system assigns the value. In a *create* form these are typically **not user-entered** (read-only/omitted); on display they're shown. This is how primary keys and `created_at` fields usually work. |
| `&`      | **Unique** — value must be unique across all rows | Editing/creating implies a uniqueness expectation; duplicate entry is a meaningful error state to design for. |
| `^`      | **Indexed** — field is indexed for faster lookup | Affects *query performance expectations*: filtering/sorting on an indexed field is cheap; on a non-indexed field it may be a full scan. Power users care which fields are indexed. |
| `?`      | **Nullable** — value may be absent (NULL)  | **Absent is distinct from empty.** `string?` can be `None` (no value) vs. `Some("")` (empty string) — these round-trip as different states. A nullable field's control needs an explicit "set/clear/null" affordance, not just an empty box. |

A field can combine modifiers (e.g. an auto-generated, unique, indexed primary key).

### 3c. Relations (how models connect — the "graph" of the data)

| Syntax     | Relationship                        | What it means for navigation |
|------------|-------------------------------------|------------------------------|
| `*Model`   | **Required foreign key** (belongs-to, non-null) | Each row points to exactly one parent row. Editing means choosing an existing target row. |
| `?Model`   | **Optional foreign key** (belongs-to, nullable) | May point to a parent or be absent. |
| `[Model]`  | **One-to-many** (has-many)          | A parent row "owns" a collection of child rows (e.g. a `User` has many `Post`s). This collection is *derived by traversal*, not stored on the row itself. |
| `[Model]`↔`[Model]` | **Many-to-many** (auto-detected when both sides list each other) | e.g. `Post` ↔ `Tag`. Backed by a junction/link table. Rows are *linked* and *traversed* in both directions. |

Navigation the data supports:
- **Forward FK:** from a row to its single parent (`post → author`).
- **Reverse one-to-many:** from a parent to its children (`user → posts`).
- **Many-to-many:** link two rows, and traverse the set in either direction (`post → tags`,
  `tag → posts`).
- **Eager load:** fetch a row *together with* its related rows in one shot (`post` +
  its `author` + its `tags`).

Design-relevant honest limits on relations (so mockups don't imply impossible things):
- Relation traversal exists only between **UUID-keyed** models (foreign keys are always UUIDs).
  Models keyed by an integer PK are not traversal targets.
- Reverse and many-to-many lookups are **linear scans**, not indexed — a design concern at
  scale (pagination, lazy loading, "this may be slow" affordances).
- **Many-to-many links can be created but not un-linked** currently (no unlink operation).

### 3d. Composite/embedded constructs

- **Inline `struct`s:** a fixed-shape value object embedded in a field (e.g. an `Address` with
  `street`/`city`/`zip`, or a `GeoLocation` with `latitude`/`longitude`). Structs contain
  **only fixed-size scalar fields** — no strings, no nested relations. In the UI they are a
  **nested group of sub-fields** within a record, editable as a unit. A struct field can be
  optional (present or absent as a whole).
- **Fixed arrays `[type; N]`:** an array with **exactly N elements** of a fixed-size type
  (e.g. `[u32; 10]`, `[char(255); 5]`). Fixed cardinality — you edit N slots, you don't
  add/remove elements.

### 3e. Directives / constraints (intent metadata on fields)

Fields can carry directives expressing **intent and validation hints**. Important caveat:
**most of these are currently semantic markers — parsed and recorded, but not runtime-enforced.**
They express what the developer *meant*, which is useful for the UI to surface (as hints,
placeholders, format expectations, badges), but the inspector should not imply enforcement the
system doesn't actually perform.

- `@min` / `@max` — numeric bounds (and `@max` doubles as a string max-length).
- `@length(min, max)` — string length range.
- `@email`, `@url` — the field is meant to hold an email / URL (format hint).
- `@pattern("regex")` / `@regex("...")` — the field is meant to match a regex.
- `@default(value)` — a default value on insert (e.g. `@default("pending")`, `@default(0)`,
  `@default(false)`). Directly useful for pre-filling create forms.
- `@index` (field) / `@index(a, b)` (model-level composite) — indexing intent; affects query
  performance expectations.
- `@computed` — the field is meant to be derived/read-only (not directly entered).
- `@fulltext` — the field is meant for full-text search.
- `@materialized` — the field is a materialized/derived value.
- `@soft_delete` (model-level) — the model opts into soft-deletion semantics.

These are a rich source of **type-aware input affordances** (min/max spinners, format
validation, defaults, "this looks like an email" hints) — but frame them as *guidance the
schema author expressed*, not guarantees.

---

## 4. What a user can DO with the data (the verb/capability surface)

This is what the inspector's controls ultimately drive. All of these exist today.

**Read**
- **Get one** record by its id/primary key.
- **List** all rows of a model, with pagination (collections can be large).
- **Filter** — query a model by a **fixed, generated set of filter predicates** (see the
  power-user section; this is *not* arbitrary SQL).
- **Traverse relations** — forward FK, reverse has-many, many-to-many, and eager-load
  (row + its relations together).

**Mutate**
- **Create** (insert) a new record.
- **Update** an existing record (replace it — whole-record, not field-level patch).
- **Delete** a record.
- **Link** two records in a many-to-many relationship.
- *Not available:* field-level partial update, cascade delete, many-to-many unlink.

**Consistency / time**
- **Snapshots:** capture a point-in-time, consistent view across all models and read from it —
  a snapshot taken *before* a change still shows the old data. Enables **time-travel / "read as
  of" / compare-against-a-snapshot** experiences.

**Live / real-time**
- **Subscribe** to a model and receive a **live-updating result set**: an initial set, then
  streamed **Added / Updated / Removed** deltas as data changes. Enables **live tail / watch /
  auto-refreshing views** driven by a filter query.

**Structural / performance (always available, at rest)**
- Per-model **row counts**, **dead-row (tombstone) counts and ratio**, per-column **types and
  byte sizes**, data/offset **file sizes**, and "compaction would reclaim N rows."
- **Raw column dump** — the raw, at-rest scalar bytes of a column *without* schema semantics
  (a low-level debugging view, explicitly "raw, no typed meaning").

**Whole-database**
- **Backup / restore** — snapshot the entire database to a backup and restore from one.
- **Schema shape / introspection** — the full structure (models, fields, types, modifiers,
  directives, relations, indexes) is available to render a **schema explorer** and a
  **relation graph**.

---

## 5. Power-user / advanced controls (design this space richly)

The user specifically wants the inspector to serve **power users**, not just click-through
browsing. Here are the dimensions where "advanced controls" live. These are **capabilities to
express**, not prescribed widgets — design them however feels right.

**Query & filter composition.** The single most important power surface.
- The user filters a model by combining **predicates over its fields** (equality, and where the
  type allows, comparisons/ranges). Crucially, the set of filterable fields and operators is a
  **closed, known set derived from the schema** — every model advertises exactly what it can be
  filtered on. There is **no free-text query language / SQL box**; the power is in *composing
  the available predicates well*.
- Advanced needs to consider: combining multiple predicates, seeing/copying the underlying
  request being sent, saving/reusing/naming a query, re-running it, sharing it, and
  understanding which predicates hit an **index** (fast) vs. a scan (slow).

**Relation drill-down.** Following the data graph fluidly: from a row to its parent, from a
parent to its children, across a many-to-many, and eager-loading a row with its relations.
Power users want to pivot ("show me all posts by *this* author") and navigate without losing
their place.

**Bulk & efficiency.** Multi-select, batch actions, keyboard-first navigation, fast paging
through large collections, inline vs. form editing, quick create/duplicate. This is where a
tool feels "pro."

**Type-aware entry & editing.** Because every field's type is known, entry controls can be
sharp: null/absent toggles for nullable fields, generate-a-uuid affordances, millisecond-aware
timestamp entry, min/max-aware numeric entry, default pre-fills from `@default`, format hints
from `@email`/`@url`/`@pattern`, nested sub-forms for embedded structs, fixed-N array editors,
and foreign-key pickers that let you choose an existing target row. **Auto-generated and
computed fields should present as system-managed, not free entry.** Big-integer (`u64`/`i64`)
precision handling is a real concern.

**Time-travel & snapshots.** Pin a snapshot, read "as of" a moment, and compare current data to
a captured snapshot. A distinctive advanced capability most DB GUIs lack.

**Live tailing.** Attach a live subscription to a filtered view and watch rows appear, change,
and disappear in real time — for debugging and monitoring.

**Storage/perf introspection for the operator.** Dead-row ratios, storage growth, compaction
reclaim estimates, index coverage, per-column sizes — the "is my database healthy?" surface,
plus the **raw column dump** for deep debugging.

**Backup/restore controls.** Create, list, and restore whole-database backups.

**Schema/graph exploration.** A structural view of models, fields, types, indexes, uniqueness,
and the relation graph — the map a power user reads before querying.

---

## 6. Boundaries the design must respect (so mockups stay honest)

- **Storage is append-only.** Updates and deletes work, but under the hood they *append* new
  versions; storage grows until a separate compaction step reclaims space. There is **no
  field-level partial update** (updates replace the whole record), **no cascade delete**, and
  **no many-to-many unlink**.
- **Typed data needs a running attached database**; structure/stats/raw/schema work at rest.
  Design for both the connected and not-connected states.
- **Filtering is a closed, generated predicate set** — not arbitrary SQL. Don't design a
  raw-query console as the primary query surface.
- **Relation traversal is UUID-keyed only**, and reverse/M2M lookups are **linear scans**
  (pagination / "may be slow" affordances matter at scale).
- **Directives are mostly intent markers, not enforced constraints** — surface them as hints,
  don't imply guaranteed validation.
- **Single-writer / single-process** model — this isn't a multi-user concurrent-editing
  environment.

---

## 7. Delivery context (light — should NOT constrain your design exploration)

The eventual implementation target is a **desktop application** that (a) reads the database's
files directly for structural/raw/schema views, and (b) talks over the local network to the
app's running generated API server for typed data, queries, mutations, and live updates. There
is an existing in-house frontend toolkit the build will likely use, but **that is an
implementation detail and should not shape your visual or interaction design** — the intent of
this handoff is to design the experience freely and organically first. Treat "desktop tool that
inspects one strongly-typed, relational, generated app database" as the only real frame.

---

## 8. Open questions worth exploring together

- How should the **connected vs. not-connected** split feel — is "attach to a running database"
  a mode, a first-run step, a per-view prompt, or backgrounded?
- What's the primary **unit of work** — browsing a model's rows, running a query, following the
  relation graph, or watching live changes? (They may be different "modes" or one unified
  surface.)
- How prominent should **time-travel/snapshots** and **live tailing** be — signature features,
  or advanced/hidden?
- How much **storage/operator** insight belongs alongside data browsing vs. in a separate
  "health" space?
- For **editing**, how do we make type-correct, constraint-aware, relation-aware entry feel
  fast for power users without overwhelming casual use?
- How should **large collections** and **linear-scan** relations be paginated/streamed so the
  tool stays responsive?

---

*Summary: you're designing a desktop inspector for a per-app, strongly-typed, relational,
generated database. The data is made of well-known scalar types, modifiers, embedded structs,
fixed arrays, and relations (§3); users can read, filter, traverse, create/update/delete,
snapshot, and live-subscribe (§4); the power lives in query composition, relation drill-down,
type-aware editing, time-travel, live tailing, and storage introspection (§5); and a handful of
append-only / closed-filter / connected-vs-at-rest boundaries keep the design honest (§6).
Everything about layout, flow, and visual language is yours to invent.*
