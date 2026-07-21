# ForgeDB v1 Roadmap — "close the core, then ship"

This is the honest state of ForgeDB's v1 effort. For the guarantees and limits in detail, see
[WHAT_V1_IS.md](./WHAT_V1_IS.md); for the live backlog, see the
[GitHub issues](https://github.com/hoodiecollin/forgedb/issues).

## The situation

ForgeDB was built **perimeter-first**: advanced features (live queries, multi-tenancy, snapshot
isolation, backup/restore, single-writer/many-reader) arrived before the core could durably store,
list, query by anything but primary key, or enforce constraints. The v1 effort closed that core.

The good news is that almost every gap was a **codegen-wiring problem**, and codegen is ForgeDB's
core competency — the hard substrate (columnar storage with flush primitives, the WAL, compaction,
query-params) already existed. Most of the work was emitting generated code that calls substrate
already there, squarely inside the generator-identity model.

## Scope (locked)

| Axis | Decision |
|---|---|
| **Release bar** | Design-partner / early-adopter — public but explicitly caveated. |
| **Concurrency** | Single-writer-per-process, documented, with a safe two-writer guard (lock/refuse, never corrupt). |
| **Migrations** | Additive changes work end-to-end; a documented workflow for breaking changes. |

## v1 core — complete

The five phases that close the core have all shipped:

- **Phase 1 — Durable write path.** The generated write path journals to the WAL, fsyncs, and
  recovers on reopen (torn-tail truncate + idempotent replay); a `DirLock` refuses a second writer.
  A bounded WAL (checkpoint → truncate) keeps reopen from replaying whole history. A `kill -9`
  writer-stress test loses zero acked rows.
- **Phase 2 — Readable database.** A real list endpoint with filter/sort/pagination (over the
  schema-agnostic `query-params` substrate and a generated closed-set matcher/comparator), plus
  generated secondary indexes with `find_by_*`/`get_by_*` probes over scalar, nullable, foreign-key,
  and composite (`@index(a, b)`) fields — an index probe, not a scan.
- **Phase 3 — Data integrity.** Generated validation (`@min`/`@max`/`@length`/`@email`/`@url`/
  `@pattern`), `&unique` enforcement, and required/optional foreign-key existence checks, rejected
  at write and at the API boundary (409 / 422).
- **Phase 4 — Bounded storage + additive evolution.** In-process auto-compaction reclaims dead row
  versions under the writer lock; additive schema changes backfill existing rows on reopen; the
  `migrate` workflow classifies additive vs. breaking changes.
- **Phase 5 — Ship.** Observability (`/health`, `/ready`, `/metrics`, structured logging), a
  container deploy story, distribution (`cargo install forgedb` from crates.io + prebuilt binaries),
  a complete typed TS SDK, a semver/stability policy, and the honest-scope docs.

Since v1 scope was locked, three items originally deferred **also** landed: multi-writer concurrency
(MVCC Tiers 1–3), the schema-migrations data-transform engine, and the cross-process durable
replication broker + coordinator. See [ARCHITECTURE.md](./ARCHITECTURE.md).

## Still deferred

- Point-in-time recovery / incremental backup
- Row-level authorization (per-principal policies within a tenant)
- Built-in JWT issuance (ForgeDB is verify-only; it does not mint tokens)

These are tracked in the [GitHub issues](https://github.com/hoodiecollin/forgedb/issues). For the
full, current account of what v1 does and does not guarantee, read
[WHAT_V1_IS.md](./WHAT_V1_IS.md).
