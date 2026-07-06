# hotel-reservations

A hotel booking system with a two-tier room model (type template vs. physical room),
date-range reservations, and payment tracking.

**Domain:** Hospitality / Hotel Management  
**Provenance:** Synthetic (modeled after the standard hotel booking design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Hotel` | Property with star rating; owns `[RoomType]` |
| `RoomType` | Pricing/capacity template (`base_price` i64 cents); owns `[Room]` |
| `Room` | Physical unit with room number and status; belongs to a `RoomType` |
| `Guest` | Guest contact details; has `[Reservation]` history |
| `Reservation` | Date-range booking linking a `Guest` to a `Room`; has `[Payment]` |
| `Payment` | Individual payment record attached to a `Reservation` |

Key relationships:
- `RoomType` sits between `Hotel` and `Room`, carrying pricing so rates can differ by class without duplicating hotel details
- `Reservation.check_in` / `check_out` timestamps define the availability window; composite `@index(room, check_in)` supports overlap queries
- A reservation can accumulate multiple `Payment` records (deposits, final charge, refunds)

## Grammar features showcased

- `+uuid` PKs and `+timestamp` `created_at` on all models
- `i64` money fields (`base_price`, `total_amount`, `amount`) for minor-unit currency (cents)
- `u32 @min(1) @max(5)` for the star-rating constraint
- `i64 @min(0)` and `i64 @min(1)` value constraints on monetary amounts
- `string?` and `i32?` nullable optional fields
- `*Model` required FK relations (type → hotel, room → type, reservation → guest/room, payment → reservation)
- `[Model]` one-to-many back-references across the hotel → room type → room → reservation chain
- Composite `@index(room, check_in)` and `@index(guest, check_in)` for date-range availability and guest history queries
