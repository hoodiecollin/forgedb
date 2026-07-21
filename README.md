<div align="center">

# ForgeDB

**An application-database generator.** Write one schema; get a tailored database.

</div>

## What it is

ForgeDB compiles a declarative `.forge` schema — at **compile time** — into a tailored
Rust database, a typed TypeScript SDK, and a REST API (with an OpenAPI 3.1 spec). It is a
code generator, **not** an ORM or query engine: your schema is a compile-time input to
generation, never a runtime input to a generic engine. The generated code is specialized
per schema over columnar storage, so there is no generic-runtime layer to pay for.

Everything in this repository is open source under `MIT OR Apache-2.0`. See
[`docs/OPEN_CORE.md`](./docs/OPEN_CORE.md) for the open-core boundary.

> **Status:** early development (0.1.x), **not yet production-ready**. What v1 actually
> guarantees — and what it defers — is stated plainly in
> [`docs/WHAT_V1_IS.md`](./docs/WHAT_V1_IS.md). Trust that over any headline here.

## One schema in, a stack out

```
User {
  id: +uuid                                              // auto-generated primary key
  email: ^&string @pattern("^[^@]+@[^@]+\\.[a-z]{2,}$")  // unique, indexed, format-checked
  username: ^&string                                     // unique, indexed
  created_at: ^timestamp                                 // indexed (ordered → range queries)
  posts: [Post]                                          // one-to-many

  @index(created_at, username)                           // composite index
}

Post {
  id: +uuid
  title: ^string @length(5, 200)                         // indexed; length-validated
  author: *User                                          // required foreign key
  view_count: ^u64                                       // indexed (ordered → range queries)
  created_at: ^timestamp
  tags: [Tag]                                            // many-to-many

  @index(author, created_at)                             // composite index
}

Tag {
  id: +uuid
  name: ^&string
  posts: [Post]                                          // many-to-many (bidirectional)
}
```

`forgedb generate` turns that into typed, schema-tailored methods — index probes,
not scans, and no runtime engine to interpret the schema (these are the real
generated signatures; probes return plain values, not a `Result`):

```rust
let ada     = db.user.get_by_email("ada@example.com");    // Option<User> — O(1) unique lookup
let popular = db.post.find_by_view_count_range(            // Vec<Post> — ordered range / top-N
                  Some(1_000), None, /* descending */ true, /* limit */ Some(10));
let recent  = db.post.find_by_author_and_created_at(author_id, ts);  // Vec<Post> — composite index
db.link_post_tag(post_id, tag_id);                        // many-to-many link
```

— plus the matching TypeScript types + client, a REST endpoint per model with
filter/sort/pagination, and the OpenAPI spec.

## How it works

```
schema.forge
    │  parser (lexer → AST) → validation
    ▼
codegen
    ├─→ Rust database code      (columnar storage, typed query API, durable writes)
    ├─→ TypeScript types + SDK  (models + API client)
    ├─→ REST API (axum)         (CRUD, relation traversal, query params)
    ├─→ OpenAPI 3.1 spec
    ├─→ browser read-replica    (WASM, opt-in — the same generated engine, in a Worker)
    └─→ migration transformer   (offline, per-version data rewrite)
```

The storage is a **columnar hybrid**: fixed-size types (`u64`, `f64`, `uuid`, …) live in
packed columns for tight, cache-friendly access; variable-length data (strings, `json`)
rides an append-only column with an offset index. Because the generated code is monomorphized
to your exact schema, there is no dynamic dispatch over a generic row type. Performance is
**measured, not asserted** — the methodology and current numbers live in
[`docs/BENCHMARKS.md`](./docs/BENCHMARKS.md).

## What's real today

Implemented and working:

- Schema parser (lexer → AST) + validation; the `forgedb` CLI
- Columnar storage engine, WAL, in-process compaction
- Crash-safe durable writes; MVCC transactions + multi-process write coordination
- Codegen: Rust database, TypeScript SDK, REST API (+ OpenAPI 3.1)
- Secondary + composite indexes, relation traversal, snapshot reads, live queries, backup/restore
- Multi-tenancy (verify-only JWT), schema migrations, browser read-replica (WASM)
- LSP server + VS Code extension; language bindings (Python / Node / Bun)

Not built (and not near-term): generated UI components. The schema can *reference* UI
components as contract markers, but ForgeDB does not generate component code today.

For the full, honest scope see [`docs/WHAT_V1_IS.md`](./docs/WHAT_V1_IS.md) and
[`docs/V1_ROADMAP.md`](./docs/V1_ROADMAP.md).

## Why generate instead of run an engine

- **One source of truth.** The schema defines storage layout, the Rust database, the TS
  types, and the API — they cannot drift, because they are all generated from it.
- **Compile-time, not runtime.** Errors surface at build time; the compiler optimizes for
  *your* schema rather than a generic one.
- **No runtime engine to interpret your schema.** Generated code links only schema-agnostic
  substrate crates (storage, WAL, types); there is no ORM reflecting over a schema at runtime.
- **Columnar from the start.** Not a row store with columns bolted on.

**Good fit:** type-safe full-stack apps with stable schemas, local-first apps (browser
read-replica), embedded use where the schema is known at compile time, services that want
strong contracts. **Poor fit:** schemas that change shape at runtime, or ad-hoc analytics
over unknown schemas.

## Getting started

Install the CLI and scaffold a project:

```bash
cargo install forgedb          # from crates.io

forgedb init my-app
cd my-app
# edit schema.forge, then:
forgedb dev                    # generate, build, and run the dev server
```

Or generate from a schema in a cloned checkout:

```bash
git clone https://github.com/hoodiecollin/forgedb && cd forgedb
cargo build --workspace
cargo run -- generate all --output ./generated   # discovers ./schema.forge
```

See [`docs/GETTING_STARTED.md`](./docs/GETTING_STARTED.md) for the full loop with verified
output, [`docs/INSTALL.md`](./docs/INSTALL.md) for every install path, and
[`examples/`](./examples/) for worked schemas across many domains.

## Documentation

**Start here**
- [Getting Started](./docs/GETTING_STARTED.md) — install → scaffold → generate → build → serve
- [Schema Language Reference](./docs/SCHEMA.md) — the complete, parser-verified `.forge` reference
- [What v1 Is — and Isn't](./docs/WHAT_V1_IS.md) — honest guarantees and limits
- [Installing](./docs/INSTALL.md) — every install path + the substrate version matrix

**Operating**
- [Deployment](./docs/DEPLOYMENT.md) — containers, env config, ops routes, multi-tenancy, JWT
- [Migrations](./docs/MIGRATIONS.md) — how schema changes affect data at rest
- [Versioning & Stability](./docs/SEMVER.md) — the compatibility policy across surfaces
- [Benchmarks](./docs/BENCHMARKS.md) — measured performance + methodology

**Internals & contributing**
- [Architecture](./docs/ARCHITECTURE.md) — system design and design decisions
- [Public Crates](./docs/PUBLIC_CRATES.md) — the schema-agnostic substrate crates
- [Contributing](./docs/CONTRIBUTING.md) · [Development](./docs/DEVELOPMENT.md) · [Publishing](./docs/PUBLISHING.md)

## Contributing

Contributions are welcome — bug fixes, tests, docs, examples, and performance work
especially. Start with the [Contributing Guide](./docs/CONTRIBUTING.md). Design proposals
are filed as [`rfc`-labeled issues](https://github.com/hoodiecollin/forgedb/issues), not
committed docs.

- **Issues:** <https://github.com/hoodiecollin/forgedb/issues>
- **Discussions:** <https://github.com/hoodiecollin/forgedb/discussions>

## License

Dual-licensed under [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.
