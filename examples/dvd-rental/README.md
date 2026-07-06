# dvd-rental

A DVD rental store: films, actors, inventory copies, customers, rentals, payments, staff, stores, and a geographic address hierarchy.

**Domain:** Physical media rental / retail operations

**Provenance:** Adapted from Sakila/Pagila DVD rental sample database
(https://github.com/devrimgunduz/pagila, PostgreSQL License — BSD-style)

---

## Models (13)

| Model | Description |
|---|---|
| `Country` | Top of the geographic hierarchy |
| `City` | City within a country |
| `Address` | Street address within a city; used by Customer, Staff |
| `Language` | Spoken language lookup (English, French, …) |
| `Category` | Film genre/category (Action, Comedy, …) |
| `Actor` | Performer; M2M with Film |
| `Film` | Movie; double Language FK (`language` + optional `original_language`); `rental_rate`/`replacement_cost` as i64 cents; `rating` as plain `string?` |
| `Store` | Rental location; optional `manager_staff: ?Staff` (mutual FK cycle) |
| `Staff` | Employee; optional `store: ?Store` (mutual FK cycle); `active: bool` flag |
| `Customer` | Renter; registered at a Store; `active: bool` flag |
| `Inventory` | One physical copy of a Film at a Store |
| `Rental` | Single rental event (inventory → customer, optional return_date) |
| `Payment` | Payment tied to a rental |

## Key Relationships

- **Geographic hierarchy:** Country → City → Address (one-to-many chain)
- **Film ↔ Actor M2M:** bidirectional `[Film]`/`[Actor]` — auto-detected many-to-many (mirrors `film_actor` join table)
- **Film ↔ Category M2M:** bidirectional `[Film]`/`[Category]` — auto-detected many-to-many
- **Double FK to Language:** Film has `language: *Language` (primary dub) and `original_language: ?Language` (nullable, for foreign films) — Language is a simple lookup with no back-reference (ambiguity from two FKs)
- **Inventory chain:** Film → Inventory → Rental → Payment
- **Mutual FK cycle (Store ↔ Staff):** `Store.manager_staff: ?Staff` and `Staff.store: ?Store` — both optional to allow either record to be inserted first, mirroring Sakila's real nullable FK pattern
- **Composite indexes:** `@index(film, store)` on Inventory; `@index(customer, rental_date)` and `@index(inventory, rental_date)` on Rental; `@index(customer, payment_date)` on Payment

## Grammar Features Showcased

- Two M2M relationships on the same model (Film ↔ Actor, Film ↔ Category) via bidirectional `[Model]`/`[Model]`
- Double FK to the same model (Film → Language twice: required + optional)
- Mutual FK cycle between two models (Store ↔ Staff, both `?Model` optional)
- Geographic one-to-many chain (Country → City → Address)
- `*Model` required FK and `?Model` optional FK
- `+uuid` primary keys, `+timestamp` auto-timestamped fields (rental_date, payment_date, created_at)
- `timestamp?` nullable timestamps (return_date, updated_at)
- `i64` for money (rental_rate, replacement_cost, payment amount — stored as cents)
- `bool` fields (active flags)
- `@min`/`@length` constraints on numeric and string fields, `@email` on contact fields
- `@default` on numeric fields (`rental_duration`, `@default(3)`)
- Multiple composite model-level indexes `@index(a, b)` per model
- `&` unique on natural keys (Language.name, Category.name, Staff.username)

## Grammar Limitation Noted

- Sakila's `film.rating` SQL ENUM (`G`, `PG`, `PG-13`, `R`, `NC-17`) is modeled as `rating: string?`. The `@pattern` directive exists in ForgeDB's grammar but only accepts unquoted identifiers as arguments — not quoted regex strings — so the allowed-values constraint cannot be expressed inline. A lookup model or application-layer validation should enforce valid ratings.
- Sakila's `special_features` TEXT[] (set of features) is modeled as a single `string?` field. ForgeDB has no array-of-string type; a separate join model would be needed to store multiple features relationally.
