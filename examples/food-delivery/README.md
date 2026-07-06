# food-delivery

An on-demand food delivery platform covering restaurant menus, order lifecycle, and
courier dispatch — with a timestamped status audit log and geographic coordinates.

**Domain:** Food Delivery / On-Demand Commerce  
**Provenance:** Synthetic (modeled after the standard food delivery design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Restaurant` | Merchant with a `GeoPoint` location; owns `[Menu]` and `[Order]` |
| `Menu` | Named menu belonging to a `Restaurant`; owns `[MenuItem]` |
| `MenuItem` | Orderable item with price (i64 cents) |
| `Customer` | Buyer with optional saved `GeoPoint` delivery location |
| `Courier` | Driver with live `GeoPoint` position and availability flag |
| `Order` | Placed order linking `Customer`, `Restaurant`, and an optional `Courier`; has `[OrderItem]` and `[OrderStatusEvent]` |
| `OrderItem` | Explicit join model (Order × MenuItem) with `quantity` and captured `unit_price` |
| `OrderStatusEvent` | Append-only timestamped audit log for order lifecycle transitions |

Key relationships:
- `Order.courier: ?Courier` is an optional FK — courier is assigned post-placement
- `OrderItem` is a join model (not bidirectional M2M) because it carries `quantity` and `unit_price` attributes
- `OrderStatusEvent` provides a full history of status changes rather than a single mutable `status` column

## Grammar features showcased

- `struct GeoPoint { latitude: f64  longitude: f64 }` — a fixed-size inline struct used on `Restaurant.location` (required), `Customer.delivery_location` (optional `GeoPoint?`), `Courier.current_location` (optional), and `Order.delivery_location` (optional)
- `StructType` vs `OptionalStructType` usage (required and nullable struct fields)
- `?Courier` optional FK (`Order.courier`) — courier assigned after placement
- Explicit join model (`OrderItem`) with two required FKs plus payload fields — contrasted with implicit M2M
- `+timestamp` for auto-timestamped `placed_at` and `changed_at` audit fields
- `i64 @min(0)` for all monetary amounts (minor currency units)
- Multiple composite `@index` entries per model (`Order`, `OrderItem`, `OrderStatusEvent`)
- `f64 @min(0) @max(5)` for the restaurant rating field
- `bool` fields for `is_active` / `is_available` flags (no `@default` — set explicitly)
