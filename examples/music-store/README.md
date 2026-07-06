# music-store

An iTunes-like digital music store with artists, albums, tracks, playlists, customers, invoices, and a support-rep employee hierarchy.

**Domain:** Digital media / e-commerce storefront

**Provenance:** Adapted from Chinook digital music store sample database
(https://github.com/lerocha/chinook-database, MIT License)

---

## Models (10)

| Model | Description |
|---|---|
| `Genre` | Music genre lookup (Rock, Jazz, Pop, …) |
| `MediaType` | File format lookup (MPEG, AAC, …) |
| `Artist` | Recording artist |
| `Album` | Album belonging to an artist |
| `Track` | Individual track; optional album and genre FKs; `unit_price` as i64 cents |
| `Playlist` | Named playlist; M2M with Track |
| `Employee` | Staff member; self-referencing `reports_to: ?Employee`; handles customer support |
| `Customer` | Buyer; optional `support_rep: ?Employee`; soft-deleted via `@soft_delete` |
| `Invoice` | Purchase header; `total` as i64 cents |
| `InvoiceLine` | Line item (invoice + track, snapshot `unit_price`, `quantity`) |

## Key Relationships

- **Catalog chain:** Artist → Album → Track (required FKs)
- **Lookup tables:** Track references `MediaType` (required) and `Genre` (optional)
- **Playlist ↔ Track M2M:** bidirectional `[Track]`/`[Playlist]` with no payload — the parser auto-detects many-to-many (mirrors Chinook's `PlaylistTrack` join table)
- **Employee self-ref:** `reports_to: ?Employee` with reverse collection `subordinates: [Employee]`
- **Customer support:** `Customer.support_rep: ?Employee` + `Employee.supported_customers: [Customer]`
- **Invoice → InvoiceLine:** one-to-many; each line captures a snapshot `unit_price` at purchase time
- **Composite indexes:** `@index(customer, invoice_date)` on Invoice; `@index(invoice, track)` on InvoiceLine

## Grammar Features Showcased

- Bidirectional `[Model]`/`[Model]` many-to-many auto-detection (Playlist ↔ Track)
- `*Model` required FK and `?Model` optional FK
- `[Model]` one-to-many reverse collections
- Self-referencing optional FK (`reports_to`/`subordinates` on Employee)
- `+uuid` primary keys, `+timestamp` created_at, `timestamp?` updated_at/nullable fields
- `&` unique on natural keys (Genre.name, MediaType.name, Artist.name, Employee.email, Customer.email)
- `i64` for money (unit_price, invoice total — stored as cents)
- `@soft_delete` model-level directive on Customer
- `@min` on numeric fields, `@length` on string fields, `@email` on contact fields
- Model-level composite indexes `@index(field_a, field_b)`
- Explicit join model with payload: `InvoiceLine` (vs. pure M2M) for order line-item pattern

## Grammar Limitation Noted

- The `@pattern` directive accepts only unquoted identifiers as arguments (no quoted string literals in the current lexer). Enum-like constraints (e.g. restricting `rating` to known values) must be expressed as a plain `string?` field with a comment noting valid values.
