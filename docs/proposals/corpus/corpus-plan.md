# ForgeDB Example Corpus — Build Plan

Location convention: `examples/<kebab-name>/schema.forge` + `examples/<kebab-name>/README.md`.
All field names snake_case (parser enforces, fatal). Models/structs PascalCase.
Every schema must pass `forgedb validate` AND `forgedb generate rust` (and `api` where it has models) — hard gate.

## Grammar features to ensure the corpus collectively exercises
- one-to-many `[Model]`, required FK `*Model`, optional FK `?Model`, bidirectional M2M `[..]`/`[..]`
- explicit join model (M2M with payload) — enrollment grade, order line qty/price
- self-reference (`manager: ?User`, `parent: ?Category`)
- `+uuid`/`+u64` PKs, `+timestamp` created_at, `timestamp?` updated_at/deleted_at
- unique `&` on natural keys; index `^`/`@index`; composite `@index(a,b)`
- constraints `@min/@max/@length/@email/@url/@pattern/@default`
- `@soft_delete`, `@fulltext`, `@computed`, `@materialized`
- structs (fixed-size only: char(N)/numeric/bool/uuid/timestamp) — geo coords, codes
- fixed arrays `[f64; N]` — sensor readings
- component refs `tsx://` `jsx://` `api://` + `@relations(...)` (snake_case field names) — done RIGHT (replaces broken component-integration)
- money as `i64` minor units; enums as `string` + `@pattern` or lookup model

## Catalog (target ~16 apps)

### Adapted (attribute source + license in README)
1. **music-store** — Chinook (MIT, Postgres). Artists→albums→tracks, genres/media types, playlists M2M, customers, invoices+lines, self-ref employee `reports_to`, money (i64 cents).
2. **dvd-rental** — Sakila/Pagila (BSD). Film↔actor M2M, film↔category M2M, inventory, rental, payment, store/staff/customer, address→city→country geo hierarchy.
3. **wholesale-orders** — Northwind (MIT port). Customers, employees (self-ref), products, suppliers, categories, orders + order_details (join w/ qty/price), shippers.
4. **hr-directory** — Oracle HR (UPL). Employee self-ref `manager`, department, job, region→country→location hierarchy, job_history (temporal).
5. **code-hosting** — Gitea (MIT). Users, orgs, repos, issues, pull_requests, labels M2M, milestones, comments, stars M2M, fork lineage self-ref.
6. **publishing-membership** — Ghost (MIT). Posts (@fulltext), authors, members, subscriptions, tiers, tags M2M, roles/permissions RBAC, post status enum.
7. **social-graph** — Mastodon (AGPL, inspiration). Accounts, statuses (self-ref reply), follows (self-ref M2M), blocks, favourites, notifications (polymorphic-ish via type + ref id), media_attachments.
8. **student-information-system** — university/SIS (teaching). Students, instructors, courses, sections, enrollment (join w/ grade payload), departments, terms; @index(student_id, section_id).
9. **healthcare** — hospital (teaching). Patients, providers, appointments, departments, prescriptions, diagnoses; role subtype via provider.kind string; @index(provider_id, scheduled_at).
10. **hotel-reservations** — hotel booking (teaching). Hotels, room_types, rooms, guests, reservations (check_in/check_out timestamps), payments; date-range availability.
11. **food-delivery** — teaching. Restaurants, menus, menu_items, customers, couriers, orders, order_items (join), order_status_events (timestamped audit log), addresses (struct geo f64 lat/long).
12. **banking-ledger** — teaching. Customers, accounts, joint ownership M2M, transactions (double-entry: debit/credit account FKs, i64 amount), transaction_type enum, statements.
13. **airline-reservations** — teaching. Airports (self-ref routes via flight), aircraft, flights, seats (unique seat lock), passengers, bookings, tickets; @index(flight_id, seat_id).

### Synthetic (invent; provenance = Synthetic)
14. **blog-cms** — canonical clean blog that REPLACES the broken component-integration example. Users, posts (@fulltext, @soft_delete), comments (self-ref parent), categories (self-ref), tags M2M, and CORRECT snake_case component refs: `profile_card: tsx://...`, `update_endpoint: api://...`.
15. **project-management** — Linear/Jira-like. Orgs, teams, projects, issues (self-ref parent, status/priority enums), sprints, labels M2M, comments, assignees.
16. **saas-multitenant** — Orgs (tenant), users, memberships (join w/ role), roles, api_keys, audit_log, invitations; tenancy + RBAC showcase.

### Optional stretch (add if time/scope allows)
17. **ecommerce-store** — modern store filling the "no official ecommerce" gap: products, variants, inventory, carts, cart_items, orders, order_items, payments, money i64 cents.
18. **iot-sensors** — high-volume: devices, sensor_readings (`+u64` pk, `reading: [f64; 3]` fixed array, timestamp), alerts; append-heavy pattern.

## Dispatch strategy (after wave 4 + rebuild)
- Parallel `forgedb-schema-author` agents, each owning a DISJOINT set of `examples/<app>/` dirs.
- Agents invoke the PREBUILT `target/debug/forgedb` binary directly (no `cargo run`) to avoid build contention; validate each schema in its own dir.
- Batch A (adapted classics): 1-5. Batch B (adapted domains): 6-10. Batch C (adapted+synthetic): 11-16. (Stretch 17-18 if pursuing.)
- Then: fix/replace component-integration, correct CLAUDE.md schema quick-ref drift (~, text, @on_delete, block comments), write examples/README.md index, consider a corpus parse-check test.
