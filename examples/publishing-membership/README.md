# publishing-membership

Blog/CMS platform with paid membership tiers, subscriptions, and RBAC for staff authors.

**Domain:** Publishing / CMS + membership + billing  
**Provenance:** Adapted from [Ghost](https://github.com/TryGhost/Ghost) data model — MIT License. No verbatim structure copied; reimagined idiomatically in `.forge`.

## Models (7)

| Model | Key fields | Relationships |
|---|---|---|
| `Author` | name, slug (unique+indexed), email (unique), status | M2M posts↔Post; M2M roles↔Role |
| `Role` | name (unique) | M2M authors↔Author |
| `Post` | title, slug (unique+indexed), content (@fulltext), status, visibility, featured | M2M authors↔Author; M2M tags↔Tag; @soft_delete; @index(status, published_at) |
| `Tag` | name, slug (unique+indexed), visibility | M2M posts↔Post |
| `Member` | email (unique), name, status | O2M Subscription |
| `Tier` | name, slug (unique+indexed), tier_type, monthly_price, yearly_price, currency | O2M Subscription |
| `Subscription` | status, cadence, amount, currency, cancel_at_period_end | `member: *Member`, `tier: *Tier`; @index(member, status) |

## Key relationships

- `Post.authors: [Author]` + `Author.posts: [Post]` — bidirectional M2M (multi-author posts)
- `Post.tags: [Tag]` + `Tag.posts: [Post]` — bidirectional M2M
- `Author.roles: [Role]` + `Role.authors: [Author]` — bidirectional M2M for RBAC
- `Subscription` — explicit join-with-payload between Member and Tier (billing cadence, amount, currency, period dates)
- Staff `Author` vs audience `Member` — two distinct user concepts, no shared model

## Grammar features showcased

- `@fulltext` on `Post.content` for full-text search indexing
- `@soft_delete` on `Post` (deleted posts become invisible without hard deletion)
- Three independent bidirectional M2M pairs in one schema
- Money as `i64` (minor currency units, e.g. cents) on `Tier` and `Subscription`
- `string @length(3, 3)` and `string? @length(3, 3)` for ISO 4217 currency codes (required and nullable variants) — currency codes are text
- `@index(status, published_at)` composite index for post feed queries
- `@index(member, status)` for subscription lookups
- `@default` with identifier literals (`draft`, `active`, `monthly`, `public`, `paid`, `free`)
- `&^` unique+indexed on slug fields across multiple models
- `timestamp?` for nullable event timestamps (`published_at`, `last_seen_at`, period dates)
