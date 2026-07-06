# saas-multitenant

A multi-tenant SaaS platform with RBAC.

**Domain:** Identity and access management — tenant organizations, user memberships, API keys, invitations, audit logging.

**Provenance:** Synthetic (invented from data-modeling knowledge).

---

## Models and key relationships

| Model | Key fields | Relations |
|-------|-----------|-----------|
| `Organization` | `slug ^&string`, `plan @default(free)` | many `Membership`, `ApiKey`, `Invitation`, `AuditLog`, `Role` |
| `User` | `email &string` | many `Membership`, `Invitation` (sent), `AuditLog` |
| `Membership` | `role @default(member)` | `*Organization`, `*User`; composite `@index(org, user)` |
| `Role` | `name &string`, `permissions` | `*Organization` |
| `ApiKey` | `key_hash &string`, `is_active` | `*Organization`, `*User` created_by |
| `Invitation` | `email @email`, `status @default(pending)` | `*Organization`, `*User` invited_by |
| `AuditLog` | `action`, `resource_type`, `resource_id uuid`, `occurred_at +timestamp` | `*Organization`, `*User` actor; `@index(resource_type, occurred_at)` |

**Relation summary:**
- `Membership` is an explicit join model (not M2M) because it carries a `role` payload
- `AuditLog.occurred_at: +timestamp` — auto-generated timestamp for immutable audit records
- `ApiKey.key_hash: &string` — unique constraint (stores the hash, not the raw key)
- `Invitation` tracks per-org, per-inviter invitations with lifecycle status

**Tenancy pattern:** Every resource (Role, ApiKey, Invitation, AuditLog, Membership) has a `*Organization` FK. Queries always scope by org, giving row-level tenant isolation.

---

## Grammar features showcased

- Explicit join model with payload: `Membership` has `*Organization`, `*User`, and `role: string` — not a simple M2M
- `uuid` scalar type on `AuditLog.resource_id` (bare uuid, not a FK relation)
- `+timestamp` auto-generate on `AuditLog.occurred_at` for immutable event timestamps
- `&string` unique on `ApiKey.key_hash` — natural key uniqueness
- `&string` unique on `Role.name` — per-name uniqueness
- Composite `@index(org, user)` on `Membership` for fast membership lookup (uses FK relation field names)
- Composite `@index(resource_type, occurred_at)` on `AuditLog` for filtered audit queries
- `timestamp?` nullable: `last_used_at`, `expires_at` on `ApiKey`
- `@default(free)`, `@default(member)`, `@default(pending)`, `@default(true)` — unquoted identifier/boolean defaults
- `@email` constraint on `Invitation.email`
