# ForgeDB Example Schema Corpus

A catalog of realistic `.forge` application schemas that exercise the ForgeDB schema
language across many domains. Each subdirectory is a self-contained example:

```
examples/<app>/
  schema.forge   # the ForgeDB schema
  README.md      # what it models, provenance, and features showcased
```

Two provenance classes: **Adapted** from a well-known open-source app or classic sample
database (attributed with source + license in each app README), and **Synthetic**
(invented from standard data-modeling patterns for that domain).

## Using an example

The CLI reads `schema.forge` from the current directory:

```bash
(cd examples/music-store && forgedb validate --strict)      # parse + semantic checks
(cd examples/music-store && forgedb generate all --output ./out)   # emit Rust/TS/API/stubs
```

Every schema here passes `validate --strict` **and** `generate all` (the `openapi` target
is skipped by design — see the root CLAUDE.md).

## Catalog (18 apps)

| App | Provenance | Models | Showcases |
|---|---|---|---|
| `hr-directory` | Adapted — Oracle HR (UPL) | 7 | Geo hierarchy, employee self-ref, mutual FK cycle, temporal job history — the small "intro" example |
| `music-store` | Adapted — Chinook (MIT) | 10 | M2M playlists, `InvoiceLine` join-with-payload, self-ref `reports_to`, `@soft_delete`, money as `i64` cents |
| `wholesale-orders` | Adapted — Northwind (MIT port) | 8 | `OrderDetail` join w/ discount, self-ref employees, multi-FK orders, composite indexes |
| `dvd-rental` | Adapted — Sakila/Pagila (BSD) | 13 | Two M2M on one model, dual FK to same model, store↔staff mutual cycle, geo chain (most complex) |
| `code-hosting` | Adapted — Gitea (MIT) | 11 | Fork lineage self-ref, PR head/base dual FK, org/team RBAC joins, issue↔label M2M |
| `publishing-membership` | Adapted — Ghost (MIT) | 7 | `@fulltext`, three M2M pairs, subscription billing join, ISO currency `char(3)` |
| `social-graph` | Inspired — Mastodon (AGPL) | 7 | Reply-thread self-ref, follow/block join models (dual FK to `Account`), notifications |
| `student-information-system` | Synthetic (teaching SIS) | 7 | Textbook M2M-with-payload (`Enrollment` grade), section/term/course FKs, GPA constraints |
| `healthcare` | Synthetic | 6 | Appointments, role-as-string providers, prescriptions/diagnoses, composite scheduling index |
| `hotel-reservations` | Synthetic | 6 | RoomType template vs Room inventory, date-range availability, `i64` money |
| `food-delivery` | Synthetic | 8 | `struct GeoPoint` (required + optional), `OrderItem` join, timestamped status-event audit log |
| `banking-ledger` | Synthetic | 6 | Double-entry transactions, `Transfer` dual FK to `Account`, joint-account M2M, `char(3)` currency |
| `airline-reservations` | Synthetic | 7 | Flight dual FK to `Airport`, unique-seat composite index (seat lock), IATA `char(3)` |
| `blog-cms` | Synthetic | 5 | **Correct snake_case component refs** (`tsx://`/`jsx://`/`api://`), self-ref comments/categories, `@fulltext`, `@soft_delete` |
| `project-management` | Synthetic | 8 | Org→Team→Project→Issue hierarchy, sub-issue self-ref, label M2M, dual composite indexes |
| `saas-multitenant` | Synthetic | 7 | Per-tenant `*Organization` scoping, `Membership` RBAC join, API keys, audit log |
| `ecommerce-store` | Synthetic | 9 | Product variants, `CartItem`/`OrderItem` joins, money as `i64` minor units, SKU/order-number natural keys |
| `iot-sensors` | Synthetic | 3 + struct | `+u64` high-volume PK (not auto-incremented yet, [#187](https://github.com/hoodiecollin/forgedb/issues/187)), fixed array `[f64; 3]`, `struct Calibration`, append-heavy telemetry |

## Provenance & licensing

Adapted examples reimagine the *data model* of their source in idiomatic `.forge` — they
copy no SQL/DDL, triggers, or code. Each app README names the source, URL, and license
(MIT, BSD, UPL, AGPL — AGPL/GPL sources are used as design inspiration only). Synthetic
examples are original, modeled after standard patterns for their domain.

## Behavioral caveat

These schemas all **parse, generate, and compile** — including nullable variable-length
strings (`string?`), inline `struct` definitions, integer PKs (`id: +u64`), and
string-literal directive arguments (`@pattern("regex")`, `@default("text")`), which earlier
revisions did not support. One behavioral caveat remains:

- **Integer `+u32`/`+u64` keys are not auto-incremented yet.** The `+` modifier synthesizes
  `+uuid` (a random UUID) and `+timestamp` (the current time) on create, but an integer auto
  key is currently a marker only — a create must supply the id. `iot-sensors` uses `id: +u64`.
  Tracked in [#187](https://github.com/hoodiecollin/forgedb/issues/187).
