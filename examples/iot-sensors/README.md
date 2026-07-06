# iot-sensors

A high-volume IoT telemetry platform for connected sensor devices.

**Domain:** Industrial/consumer IoT — device registry, 3-axis sensor readings, alerts, device calibration.

**Provenance:** Synthetic (invented from data-modeling knowledge).

---

## Models and key relationships

| Model | Key fields | Relations |
|-------|-----------|-----------|
| `Device` | `serial_number ^&string`, `calibration Calibration?` | many `SensorReading`, `Alert` |
| `SensorReading` | `id +u64`, `reading [f64; 3]`, `recorded_at +timestamp` | `*Device`; composite `@index(device, recorded_at)` |
| `Alert` | `severity`, `message`, `raised_at +timestamp`, `resolved_at?` | `*Device` |

**Struct:**
```
struct Calibration {
  offset: f64
  scale: f64
}
```
Used as an optional embedded value type on `Device.calibration`.

---

## Grammar features showcased

- **`+u64` primary key** on `SensorReading` — u64 auto-increment for high-volume append-only tables (avoids UUID entropy overhead at scale)
- **Fixed array `[f64; 3]`** on `SensorReading.reading` — three-axis (x, y, z) sensor sample in a single typed field
- **`struct Calibration`** — fixed-size embedded value type (f64 fields only, no strings); used as `calibration: Calibration?` (nullable struct) on `Device`
- **Composite `@index(device, recorded_at)`** using a FK relation field name — validated by the parser against declared field names
- `^&string` unique + indexed on `serial_number` — unique device identifier
- `f64?` nullable for optional GPS coordinates (`location_lat`, `location_lon`)
- `timestamp?` nullable for `last_seen_at` (new device, not yet seen) and `resolved_at` (unresolved alert)
- `+timestamp` auto-generate on both `Device.registered_at` and `Alert.raised_at`
- Lean schema (3 models) demonstrating that useful apps need not be large

---

## Grammar limitation noted

The `struct` type may only contain fixed-size scalars. Variable-length `string` fields are not allowed inside structs. `Calibration` uses `f64` for both fields, which is valid. If richer device metadata is needed, it must be modeled as a separate `Model` with FK relation, not as a struct.
