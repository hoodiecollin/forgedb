# airline-reservations

An airline reservation system covering airports, aircraft, flights, seat inventory,
passenger bookings, and issued tickets — with a unique-seat-lock constraint modeled
via composite index.

**Domain:** Airline / Travel Reservation  
**Provenance:** Synthetic (modeled after the standard airline reservation design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Airport` | Airport with a unique IATA code (`string(3!)` — inline, exactly 3 chars) |
| `Aircraft` | Airframe with capacity; owns `[Seat]` and `[Flight]` |
| `Flight` | Scheduled service with two `*Airport` FKs (origin + destination) and an `*Aircraft` FK |
| `Seat` | Physical seat on an aircraft; composite index prevents duplicate seat numbers per aircraft |
| `Passenger` | Traveler with passport and contact details |
| `Booking` | Reservation linking `Passenger`, `Flight`, and `Seat`; composite index enforces the seat lock |
| `Ticket` | Issued travel document with price (i64 cents) and fare class |

Key relationships:
- `Flight` holds two required FK references to the same `Airport` model (`origin_airport` and `destination_airport`) — the classic dual-FK self-referential pattern for route modeling
- `Seat.@index(aircraft, seat_number)` enforces per-aircraft seat uniqueness
- `Booking.@index(flight, seat)` is the no-double-booking seat lock — each (flight, seat) combination appears at most once in active bookings
- `Booking` one-to-many `[Ticket]` supports reissue/amendment scenarios

## Grammar features showcased

- `^&string(3!)` on `Airport.iata_code` — an indexed, unique, fixed-width inline string. IATA codes are text, not bytes, so `bytes(3)` would put `[83, 70, 79]` on the wire; and the length belongs in the type rather than in a `@length(3, 3)` directive, which is why the directive is rejected here
- Dual FK to the same model (`Flight.origin_airport: *Airport`, `Flight.destination_airport: *Airport`)
- Composite `@index(aircraft, seat_number)` on `Seat` — unique seat per airframe
- Composite `@index(flight, seat)` on `Booking` — the discrete seat-lock reservation constraint
- Multiple composite indexes on `Booking` (`@index(flight, seat)` and `@index(passenger, booked_at)`)
- `*Model` required FK on three fields of `Booking` simultaneously
- `i64 @min(0)` for ticket price (minor currency units)
- `string?` nullable `passport_number`, `date_of_birth`, and `fare_class`
- `+timestamp` auto-generated `booked_at` and `issued_at` fields
