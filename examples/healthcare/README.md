# healthcare

A hospital/clinic management system covering clinical operations from scheduling
through prescribing and diagnosis documentation.

**Domain:** Healthcare  
**Provenance:** Synthetic (modeled after the standard healthcare management design pattern)

## Models and key relationships

| Model | Role |
|---|---|
| `Department` | Organizational unit; one-to-many `Provider` |
| `Provider` | Doctor, nurse, or technician (role via `kind` string); belongs to a `Department` |
| `Patient` | Demographics and contact info; root for clinical records |
| `Appointment` | Scheduled encounter between a `Patient` and a `Provider`; carries optional `vitals: Vitals?` |
| `Prescription` | Drug order authored by a `Provider` for a `Patient` |
| `Diagnosis` | ICD-style code record attached to an `Appointment` |

Key relationships:
- `Appointment` carries two required FKs (`patient: *Patient`, `provider: *Provider`) plus a one-to-many back to `[Diagnosis]`
- `Prescription` also bridges both `Patient` and `Provider`
- `Diagnosis` hangs off `Appointment` (appointment FK only; patient is reachable through it)

## Grammar features showcased

- `+uuid` primary keys and `+timestamp` auto-generated timestamps on every model
- `&string @email` unique email fields on both `Provider` and `Patient`
- `^string` indexed field (`code` on `Diagnosis` for fast ICD lookup)
- `string?` nullable optional fields (`phone`, `notes`, `description`)
- `*Model` required FK and `?Model` optional FK relations
- `[Model]` one-to-many virtual back-references
- Composite `@index(provider, scheduled_at)` and `@index(patient, scheduled_at)` on `Appointment` — the key query pattern for provider schedules and patient history
- `@min(N)` and `@length(N, M)` constraints documenting domain rules
- `struct Vitals { systolic, diastolic, heart_rate }` embedded as `Appointment.vitals: Vitals?` — three fixed-width numbers that belong together, optional because not every visit records them. A struct may hold only fixed-size scalars, which is exactly what vitals are
- Provider role modeled as a `string` field (`kind`) with `@length` constraint. A declared `enum` would also fit (see `food-delivery`'s `OrderStatus`); the string is kept here to show the other idiom
