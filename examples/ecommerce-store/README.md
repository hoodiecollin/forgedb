# ecommerce-store

A modern e-commerce product catalog, cart, and order system.

**Domain:** Retail commerce — products, variants, inventory, customers, carts, orders, payments.

**Provenance:** Synthetic (invented from data-modeling knowledge).

---

## Models and key relationships

| Model | Key fields | Relations |
|-------|-----------|-----------|
| `Product` | `slug ^&string`, `category`, `is_active` | many `ProductVariant` |
| `ProductVariant` | `sku ^&string`, `price i64` (cents) | `*Product`, many `Inventory` |
| `Inventory` | `quantity u32`, `reserved u32`, `warehouse` | `*ProductVariant` |
| `Customer` | `email &string` | many `Cart`, `Order` |
| `Cart` | `created_at`, `updated_at` | `*Customer`, many `CartItem` |
| `CartItem` | `quantity u32`, `added_at` | `*Cart`, `*ProductVariant` (join with payload) |
| `Order` | `order_number ^&string`, `status`, `total i64` (cents) | `*Customer`, many `OrderItem`, `Payment`; `@index(status, created_at)` |
| `OrderItem` | `quantity u32`, `unit_price i64` (cents) | `*Order`, `*ProductVariant` (join with payload) |
| `Payment` | `amount i64` (cents), `method`, `status` | `*Order` |

**Relation summary:**
- `CartItem` is an explicit join model (not M2M) — carries `quantity` payload linking `Cart` and `ProductVariant`
- `OrderItem` is an explicit join model — carries `quantity` and `unit_price` (price at time of order, not live price)
- `ProductVariant.price` and `OrderItem.unit_price` both stored as `i64` cents (no float precision loss)
- One order can have multiple `Payment` records (partial payments, refunds)

---

## Grammar features showcased

- **Money as `i64` minor units (cents):** `price`, `total`, `amount`, `unit_price` — avoids float imprecision
- **Explicit join models with payload:** `CartItem` (quantity) and `OrderItem` (quantity + unit_price) — not simple M2M
- `^&string` unique + indexed on `sku` and `order_number` — natural key uniqueness
- `u32` for quantities and counts (non-negative semantics)
- `i32?` nullable for optional weight
- `@min(0)` on money fields; `@min(1)` on quantity fields
- `@default(0)` on `Inventory.quantity` and `reserved`
- `@default(true)` on `is_active` fields
- `@default(pending)` on `status` fields
- `@email` on `Customer.email`
- `@length` constraints on names, slugs, SKUs
- Composite `@index(status, created_at)` on `Order`
- `string?` nullable: `description`, `phone`, `notes`, `transaction_id`
- `timestamp?` nullable `updated_at` on mutable entities
