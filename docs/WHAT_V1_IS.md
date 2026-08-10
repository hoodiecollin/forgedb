# What ForgeDB v1 Is — and Isn't

An honest account of the guarantees and limits of ForgeDB v1. ForgeDB is built
**perimeter-first**: the advanced, well-documented features (live queries,
multi-tenancy, snapshots, backup, single-writer/many-reader) are real, and they
sit on a generated core that v1 brought to a **design-partner bar** — solid and
crash-safe, with a small number of deliberate limits called out below. When a
maturity claim matters to you, verify it against the code; this document exists
so you don't have to guess.

The target for v1 is a **design partner**: someone who can install, scaffold,
deploy, use the typed SDK, and operate a real app within the contract below —
not yet an unattended, arbitrary-scale, multi-writer deployment.

---

## The core contract

**Single writer per data directory.** A generated process takes an advisory
directory lock on open; a second writer against the same directory refuses to
start rather than corrupt. Concurrent **readers** are fully supported (lock-free,
snapshot-consistent). Scale one dataset **vertically** (one writer) and scale
**across** tenants horizontally (one process per tenant). Multi-writer
coordination is explicitly out of v1.

**The schema is a compile-time input, never a runtime one.** Every
schema-tailored thing — types, tables, queries, filters, relations, API routes,
validation — is *generated* per app at compile time. Nothing ForgeDB ships reads
your `.forge` at runtime. The published `forgedb-*` crates your app links are
schema-agnostic substrate (storage, WAL, types, change-feed, auth, query-params,
compaction); they know nothing about any specific schema. This is the identity
invariant (`CLAUDE.md`, [ARCHITECTURE.md](./ARCHITECTURE.md)).

---

## What v1 guarantees

### Durable, crash-safe writes
Generated `insert`/`update`/`delete` journal an opaque row blob to a per-model
WAL with an fsync **before** any column is touched. On reopen, recovery truncates
a torn column tail to the last consistent prefix and idempotently replays the WAL
tail. A `kill -9` mid-write loses **zero acknowledged rows**. The WAL is bounded:
a generated `checkpoint()` fsyncs columns then truncates the WAL, auto-invoked on
an interval, so recovery never replays whole history.

### Readable, queryable data
The REST `list` endpoint fetches live rows and supports filter (`?<field>=`),
sort (`?sort=`), and pagination (`?limit&offset`), returning
`{data,total,limit,offset}`. Indexed scalar fields (`^`), foreign keys, composite
`@index(a,b)`, and nullable fields get in-memory indexes with O(1)
`find_by_*`/`get_by_*` probes (not scans). Relation traversal — forward FK,
reverse one-to-many, many-to-many, and eager-load — is generated.

### Enforced data integrity
Writes refuse to commit a record that breaks integrity, all generated per-field:
`@min`/`@max`/`@length`/`@email`/`@url` field constraints (→ **422**), `&unique`
(→ **409**), and foreign-key existence for `*Target`/`?Target` (→ **409**). Both
the Rust API and the REST layer enforce these (REST create/update route through
the integrity-checking wrappers).

### Bounded storage
Update/delete leave dead row versions (append-only); a generated `compact()`
reclaims them in-process under the single-writer lock, auto-invoked at a dead-row
threshold. No background thread (that would fight the writer's directory lock).

### Snapshots, readers, and change feeds
Lock-free point-in-time read snapshots (from an append-only watermark, no version
machinery); read-only reader handles for concurrent readers under a live writer;
an in-process change-feed with typed insert/update/delete events, WebSocket
subscriptions, and stateful live queries.

### Multi-tenancy (physical)
Dir-per-tenant isolation with a verify-only JWT tenant guard — one process serves
one tenant (see [DEPLOYMENT.md](./DEPLOYMENT.md)).

### Distribution & tooling
`cargo install forgedb` from crates.io; prebuilt binaries via the release
workflow; a full-CRUD typed TypeScript SDK; observability routes; a container
deploy path; and a stated [semver policy](./SEMVER.md).

---

## What v1 is *not* (deferred, by design)

### Concurrency
- **No multi-writer.** One writer per data directory is the contract. Concurrent
  writers / a transaction manager / version chains are a future direction.
- **Single-process.** The change-feed, live queries, and snapshots are
  in-process; there is no cross-process broker or distributed coordination.

### Migrations
- **Additive-only automatic path.** Adding a model or a nullable/appended field
  is data-preserving on reopen. **Breaking** changes (type change, field/model
  removal, non-null add, adding `&unique`) require the documented **dump →
  regenerate → reload** path — there is **no runtime data-transform engine** in
  v1. `forgedb migrate create --auto` accepts additive deltas and **refuses**
  breaking ones with a non-zero exit. New non-null fields backfill to type-zero
  (not `@default`) and must be appended at the end. See
  [MIGRATIONS.md](./MIGRATIONS.md).

### Auth
- **Verify-only.** ForgeDB verifies asymmetric JWTs your identity provider issues
  and cross-checks a tenant claim; it **does not issue tokens** (deferred)
  and does **not** do row-level / per-principal authorization (deferred).
  The guard is tenant-level: right tenant → allowed, wrong tenant → 403.

### API / SDK
- The REST **create** body is the whole record — you supply auto (`+`) fields
  (`id`, `created_at`) and virtual relation fields (as `null`). The direct Rust
  `db.create_<model>` path auto-generates `+` fields; the REST layer does not
  yet. `create` returns the new id, not the full record.
- The REST **update** body is likewise the whole record — **`PUT` replaces, and there
  is no `PATCH`**. Changing one field means GET → mutate → `PUT`, and because there
  is no conditional-request support either (no `ETag` / `If-Match`), two concurrent
  editors of the same row silently clobber each other's other fields. The generated
  SDKs have the same shape (`update<Model>(id, data: Model)`). Partial update and
  preconditions are tracked as [#278](https://github.com/hoodiecollin/forgedb/issues/278).
- SDK ids are typed `string` uniformly (integer-PK callers pass `String(n)`).
- No WebSocket/subscription client in the generated SDK yet (REST only).

### Query capability
- Scalar / foreign-key / composite (`@index(a, b)`) indexes are **hash
  exact-match** — they answer `field = value` and `a = ? AND b = ?`, not prefix
  matches. Ordered-eligible fields (`u32`/`u64`/`i32`/`i64`/`timestamp`/`decimal`)
  *additionally* get a parallel ordered (BTreeMap) index, so they answer range and
  top-N queries via `find_by_<field>_range(min, max, descending, limit)`. Still
  deferred: prefix search, `f64`/nullable ordered indexes, and the snapshot (`_at`)
  ordered form. Many-to-many junction lookups are still linear scans (and
  `unlink_<a>_<b>` / `unlink_all_*` is likewise a linear junction scan).

### Operations
- `/metrics` is minimal JSON (per-model row counts), **not** Prometheus text
  format.
- Generated constants **are** configurable, but only at **generate time** — the
  `forgedb.toml` `[runtime]`/`[storage]` sections set the fsync policy, WAL
  checkpoint interval, compaction threshold, change-feed capacity, cascade depth,
  transaction retries and page limits (epic #126). What is deferred is **runtime**
  mutability: a value is either specialized-away or baked as a `const`, so
  retuning one means regenerating and rebuilding, and a live process cannot be
  retuned at all.

### Not a general-purpose engine
- No generic/dynamic query builder or ORM that interprets an arbitrary schema at
  runtime. A *generated*, schema-tailored query/filter builder is fine and
  present; a schema-agnostic runtime one is explicitly rejected (identity
  invariant).

---

## Reading the maturity claims

The [V1_ROADMAP.md](./V1_ROADMAP.md) and the project's GitHub issues carry the
authoritative, code-grounded status of every feature and its honest limits.
Where a marketing-style description in the top-level README seems to promise more
than this document, **this document and the roadmap are correct**.
