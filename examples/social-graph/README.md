# social-graph

Federated microblogging server with accounts, status posts, follow/block graphs, and notifications.

**Domain:** Federated social network / microblogging  
**Provenance:** Inspired by the [Mastodon](https://github.com/mastodon/mastodon) data model — AGPL-3.0. Data model inspiration only; no verbatim structure copied. Not a derivative work.

## Models (7)

| Model | Key fields | Relationships |
|---|---|---|
| `Account` | username (indexed), domain (nullable=local), uri, is_local, follower/following/status counts | O2M Status, MediaAttachment, Favourite |
| `Status` | content (@fulltext), visibility, sensitive, language (`string?`, exactly 2 chars), in_reply_to | `account: *Account`; `in_reply_to: ?Status` (self-ref); O2M MediaAttachment, Favourite; @soft_delete |
| `Follow` | show_reblogs, notify | `follower: *Account`, `followed: *Account` (two FKs to same model); @index(follower, followed) |
| `Block` | — | `blocker: *Account`, `blocked: *Account`; @index(blocker, blocked) |
| `Favourite` | — | `account: *Account`, `status: *Status`; @index(account, status) |
| `Notification` | notification_type, read | `account: *Account` (recipient), `from_account: *Account` (sender), `status: ?Status`; @index(account, read) |
| `MediaAttachment` | media_type, url, width, height, blurhash | `account: *Account`, `status: ?Status`; @index(status, created_at) |

## Key relationships

- `Status.in_reply_to: ?Status` — self-referential optional FK for reply threads
- `Follow.follower / followed: *Account` — explicit join model for account→account graph; two FKs to same model
- `Block.blocker / blocked: *Account` — same pattern for the block graph
- `Notification.account / from_account: *Account` — two FKs to Account (recipient + sender)
- `Account.domain: string?` — `NULL` = local account, non-null = remote federated account
- Bidirectional references avoided for dual-FK models to prevent O2M ambiguity

## Grammar features showcased

- `?Status` self-referential optional FK (`in_reply_to`) on `Status`
- Two FKs to the same model in explicit join models (`Follow`, `Block`, `Notification`)
- `@soft_delete` on `Status` (federated deletions propagate asynchronously)
- `@fulltext` on `Status.content`
- `string? @length(2, 2)` nullable language-code field — a BCP 47 subtag is text
- `@index` on both FK fields of join models for bidirectional graph traversal
- `@index(account, read)` for unread-notification queries
- `@index(status, created_at)` for chronological media attachment lookups
- Denormalized counter fields (`followers_count`, `following_count`, `statuses_count`)
- Optional FK `status: ?Status` on `MediaAttachment` and `Notification`

## Grammar limitation noted

`@pattern` with regex cannot be expressed in the current parser (constraint params accept only numeric literals and plain identifiers, not quoted strings with special characters). Status string fields like `visibility` and `notification_type` carry semantic intent in comments rather than enforced patterns.
