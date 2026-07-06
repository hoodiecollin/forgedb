# OSS Application Database Schemas — Inspiration Catalog for ForgeDB Examples

Research corpus of real open-source application data models (Postgres-focused) to serve as
**inspiration** for hand-authored ForgeDB `.forge` example schemas. ForgeDB is a compile-time
DB generator with its own declarative schema language — these are NOT to be copied as SQL or
migrations; the goal is to catalog the data models and reimagine them cleanly.

All facts pulled from primary sources (repo schema/model/migration files + `LICENSE` files +
official docs). License notes matter: treat every entry as design inspiration only; several are
copyleft (AGPL/GPL) or source-available (BSL) and must not ship verbatim-derived structure.

Domain slots covered: forum, federated social, publishing/CMS+billing, scheduling, support inbox,
e-commerce (x2), team chat, git/code hosting, wiki/docs, e-signature, CRM (metadata-driven),
surveys, analytics SaaS, auth/RLS.

---

## 1. Discourse — Forum / community platform

- **Domain:** Forum / community discussion (topics, posts, categories, trust levels).
- **Repo:** https://github.com/discourse/discourse (Ruby on Rails; schema `db/schema.rb`).
- **License:** **GPL-2.0** (copyleft — inspiration only).

**Core data-model sketch**

| Entity | Key fields | FKs / relationships |
|---|---|---|
| `users` | username, email, trust_level, admin, moderator | 1:1 user_profiles, user_stats |
| `groups` / `group_users` | name, automatic, grant_trust_level; owner, notification_level | user↔group join |
| `categories` | name, slug, **parent_category_id**, topic_count, read_restricted | self-ref tree |
| `topics` | title, slug, archetype, posts_count, views, **deleted_at/deleted_by_id** | → users, categories |
| `posts` | topic_id, post_number, raw, cooked, reply_to_post_number, like_count, **deleted_at**, version | → topics, users |
| `post_revisions` | post_id, user_id, modifications (json), number, hidden | edit history |
| `tags` / `topic_tags` | name, topic_count | many-to-many |
| `tag_groups` / `tag_group_memberships` | name, **parent_tag_id**, one_per_topic | tag hierarchy |
| `topic_users` / `tag_users` / `category_users` | last_read_post_number, notification_level, liked, bookmarked | per-user prefs / engagement |
| `post_actions` | post_id, user_id, post_action_type_id, agreed_at/disagreed_at/deleted_at | likes/flags + moderation |
| `notifications` | user_id, notification_type, data (json), read, topic_id | polymorphic-ish payloads |
| `badges` / `user_badges` | name, badge_type_id, grant_count, multiple_grant | gamification |

**Standout patterns:** hierarchical tree data (self-ref `categories.parent_category_id`,
`tag_groups.parent_tag_id`); soft-delete with actor (`deleted_at` + `deleted_by_id`); heavy
denormalized counters (`topic_count`, `posts_count`, `like_count`); trust-level/role model
(integer trust_level + admin/moderator flags + groups); tagging (M2M + tag groups); versioning
(`post_revisions`); moderation state machine on `post_actions`; JSON notification payloads.

**ForgeDB take:** Excellent **forum/community** slot — best OSS demo of self-referential
hierarchies + soft-delete-with-audit + denormalized counters; trust-level = clean enum-role demo.

> Caveat: `db/schema.rb` did not resolve via raw fetch; table/column details came from a
> third-party Postgres mirror (prisma/database-schema-examples). Verify exact current columns
> against the live `db/schema.rb` before hard-coding.

---

## 2. Mastodon — Federated social network

- **Domain:** Federated (ActivityPub) social network / microblogging server.
- **Repo:** https://github.com/mastodon/mastodon (Rails; live `db/schema.rb`).
- **License:** **AGPL-3.0** (strong network copyleft — inspiration only).

**Core data-model sketch**

| Entity | Key fields | FKs / relationships |
|---|---|---|
| `accounts` | username, **domain** (NULL=local), public_key, uri, actor_type, suspended_at, silenced_at, **moved_to_account_id** | self-ref |
| `users` | account_id, email, encrypted_password, confirmed_at, approved, role_id | 1:1 → accounts; → user_roles |
| `user_roles` | name, **permissions (bitmask)**, position, color | RBAC |
| `statuses` | account_id, text, visibility, sensitive, **deleted_at**, edited_at, uri, **in_reply_to_id**, **reblog_of_id**, poll_id | self-ref reply/reblog |
| `media_attachments` | account_id, status_id, type, remote_url, blurhash, description | → accounts, statuses |
| `follows` / `follow_requests` / `blocks` / `mutes` | account_id, target_account_id, uri | account↔account graph |
| `favourites` / `mentions` | account_id, status_id | → accounts, statuses |
| `tags` / `statuses_tags` | name, listable, trendable, **reviewed_at** | M2M + moderation |
| `notifications` | account_id, from_account_id, **activity_id + activity_type (polymorphic)**, type, group_key | polymorphic |
| `polls` / `poll_votes` | account_id, poll_id, choice, uri | → accounts |
| `domain_blocks` / `account_domain_blocks` | domain, severity, reject_media | federation controls |
| `custom_emojis` / `tombstones` | shortcode, **domain**, uri; account_id, by_moderator | remote-aware / deletion records |

**Standout patterns:** federation as first-class (`uri`/`url` + nullable `domain` = local-vs-remote
partition everywhere); extensive self-referential graphs (follows/blocks/mutes; reply/reblog
chains; account `moved_to_account_id`); polymorphic association (`notifications.activity_type/id`);
soft-delete + edit history; RBAC via `user_roles` permissions bitmask; moderation state machines
(suspend/silence timestamps, tag `reviewed_at`); i18n (`follows.languages`, per-status language).

**ForgeDB take:** Premier **social graph / federated network** slot — unmatched for self-referential
M2M relationships, polymorphic associations, and a local-vs-remote (`domain`/`uri`) partition; also
a clean bitmask-RBAC example.

---

## 3. Ghost — Publishing / CMS + membership + newsletters

- **Domain:** Blog/CMS + paid membership + email newsletters.
- **Repo:** https://github.com/TryGhost/Ghost (Node.js; schema `ghost/core/core/server/data/schema/schema.js`).
- **License:** **MIT** (most permissive of the batch — safest to draw from).

**Core data-model sketch**

| Entity | Key fields | FKs / relationships |
|---|---|---|
| `posts` | uuid, title, slug, status (draft/published/scheduled/sent), visibility, type, featured, newsletter_id | → newsletters |
| `posts_authors` / `posts_tags` | post_id + author_id/tag_id, sort_order | M2M joins |
| `tags` | name, slug, visibility (public/internal) | — |
| `users` (staff) | name, email, password, status (active/inactive/locked), visibility | via roles_users |
| `roles` / `roles_users` | name; role_id + user_id | RBAC |
| `permissions` / `permissions_roles` / `permissions_users` | name, object_type, action_type | RBAC leaf + joins |
| `members` (audience) | uuid, email, name, status (free/paid/comped), email_disabled, last_seen_at | distinct from staff `users` |
| `products` (tiers) | name, type (paid/free), currency, monthly_price, yearly_price, trial_days | — |
| `subscriptions` | type, status (active/canceled/expired), member_id, tier_id, cadence, **amount, currency**, offer_id | → members, products, offers |
| `members_products` | member_id, product_id, expiry_at | cascade delete |
| `members_stripe_customers[_subscriptions]` | member_id, customer_id, mrr, current_period_end | Stripe bridge |
| `offers` | name, code, product_id, discount_type (percent/amount/trial), duration | → products |
| `newsletters` / `members_newsletters` | name, slug, status, sender_email; opt-in | — |
| `emails` / `email_recipients` / `email_batches` | post_id, status, recipient_filter, opened_count; delivered_at/opened_at | send tracking |
| `post_revisions` / `mobiledoc_revisions` | post_id, lexical/mobiledoc, author_id, reason | versioning |
| `members_created_events` / `members_status_events` | member_id, attribution_type, utm_*, from/to_status | audit / status log |

**Standout patterns:** full RBAC matrix (users→roles_users→roles→permissions_roles→permissions +
direct permissions_users); two distinct people tables (`users` staff vs `members` audience);
money/subscriptions (currency + amount + tiers + Stripe mirror + MRR); versioning (Lexical +
legacy mobiledoc); status-enum state machines everywhere with `members_status_events` audit log;
UUIDs alongside integer PKs; UTM attribution event tables; key/value `settings` bag.

**ForgeDB take:** Ideal **SaaS publishing / membership + billing** slot — best for RBAC join-table
chains, money/subscription modeling, and status-enum state machines with event-log audit; MIT.

---

## 4. Cal.com — Scheduling / booking

- **Domain:** Scheduling infrastructure (Calendly alternative): event types, availability, bookings.
- **Repo:** https://github.com/calcom/cal.com (Prisma; `packages/prisma/schema.prisma`).
- **License:** **AGPLv3** core + **commercial license** for Enterprise Edition (`packages/features/ee/`).

**Core data-model sketch**

| Entity | Key fields | FKs / relationships |
|---|---|---|
| `User` | email, username, timeZone, locale, identityProvider (CAL/GOOGLE/SAML/AZUREAD) | owns EventTypes, Schedules, Bookings |
| `Team` | slug, name, **parentId** (self-ref org→team), isOrganization | org hierarchy |
| `Membership` | userId, teamId, role (MEMBER/ADMIN/OWNER), customRoleId | user↔team join |
| `Profile` | userId, organizationId, username | org-scoped identity |
| `EventType` | title, slug, length, schedulingType (ROUND_ROBIN/COLLECTIVE/MANAGED), JSON bookingFields | owned by User or Team |
| `Schedule` / `Availability` | timeZone; days[], startTime, endTime, date? | weekly rule OR single date |
| `Host` | userId + eventTypeId (composite), isFixed, priority, weight, scheduleId? | round-robin routing |
| `Booking` | uid (unique), eventTypeId, startTime/endTime, status (ACCEPTED/PENDING/CANCELLED/REJECTED/AWAITING_HOST), JSON responses | → EventType, User |
| `Attendee` | bookingId, email, name, timeZone, noShow | → Booking |
| `Payment` | bookingId, **amount, currency**, success, refunded, paymentOption | → Booking |
| `Credential` | userId?/teamId?, type, encrypted JSON key, appId | user- or team-scoped |
| `Webhook` | subscriberUrl, eventTriggers[], userId?/teamId?/eventTypeId?, secret | polymorphic scope |
| `Role` / `RolePermission` | resource, action | custom RBAC |
| `BookingAudit` / `AuditActor` | immutable, soft (non-FK) refs | audit trail |
| `EventTypeTranslation` | field, sourceLocale, targetLocale, translatedText | i18n |

**Standout patterns:** multi-tenancy (Team/Org self-ref `parentId` + per-org `Profile`); RBAC (enum
roles + pluggable `Role`/`RolePermission`); state machine (`Booking.status`); polymorphic scoping
(Webhook/Credential → user OR team OR event type); i18n translation table; immutable audit with
deliberately unconstrained FKs; composite keys (Host); money (`Payment`); idempotency keys.

**ForgeDB take:** Dense **scheduling / multi-tenant SaaS** slot — hierarchical orgs, join tables,
enums-as-state-machines, audit logging. Large; distill ~12 tables, not all.

---

## 5. Chatwoot — Customer support / shared inbox

- **Domain:** Customer engagement / shared-inbox helpdesk (Intercom/Zendesk alternative).
- **Repo:** https://github.com/chatwoot/chatwoot (Rails; `db/schema.rb`).
- **License:** **MIT** core + separately-licensed `enterprise/` directory.

**Core data-model sketch**

| Entity | Key fields | FKs / relationships |
|---|---|---|
| `accounts` | name, locale, domain, status, jsonb settings/limits/feature_flags | tenant root; per-account seq for display_id |
| `users` | email (global), encrypted_password, otp_secret, jsonb ui_settings | joined via account_users |
| `account_users` | account_id, user_id, role (int enum), custom_role_id?, availability | RBAC/tenancy pivot |
| `inboxes` | account_id, **channel_id + channel_type (polymorphic)**, enable_auto_assignment, csat_survey_enabled | → channel_* table |
| `inbox_members` | inbox_id, user_id | agent↔inbox |
| `contacts` | account_id, name, email, phone_number, identifier, company_id?, jsonb custom_attributes | per-account unique email |
| `contact_inboxes` | contact_id, inbox_id, source_id, hmac_verified, pubsub_token | bridge |
| `conversations` | account_id, inbox_id, display_id (per-account seq), contact_id, assignee_id?, team_id?, sla_policy_id?, status, priority | state machine |
| `messages` | conversation_id, content, message_type, **sender_type+sender_id (polymorphic)**, private, jsonb content_attributes/sentiment | User or Contact sender |
| `attachments` | message_id, file_type, external_url, jsonb meta | → messages |
| `teams` / `team_members` | account-scoped agent groups | join |
| `labels` | account-scoped title/color | unique per account |
| `tags` / `taggings` | **taggable_type/id + tagger_type/id (polymorphic)**, context | acts-as-taggable |
| `custom_attribute_definitions` | attribute_key, attribute_model, attribute_display_type, jsonb attribute_values | user-defined schema |
| `channel_*` (web_widget/email/api/whatsapp/sms/telegram/...) | provider credentials/tokens | referenced polymorphically by inboxes |
| `sla_policies` / `applied_slas` / `audits` | thresholds; polymorphic audit-trail | — |

**Standout patterns:** textbook multi-tenancy (`account_id` everywhere + per-account sequences);
heavy polymorphic associations (inbox↔channel, message sender, taggings, audits); RBAC
(`account_users.role` + custom role); state machines (`conversation.status`); pervasive jsonb;
tagging via acts-as-taggable; audit trail; app-level (no DB CASCADE) relationships; pgvector AI cols.

**ForgeDB take:** Strongest **multi-tenant SaaS + polymorphism** slot — best demo of tenant scoping,
polymorphic channel refs, and tagging in one schema. Distill ~15 core + 2-3 channel tables.

---

## 6. Medusa — Headless e-commerce (modular)

- **Domain:** Modular headless commerce (Shopify-backend alternative); each domain an isolated module.
- **Repo:** https://github.com/medusajs/medusa (v2 models `packages/modules/<module>/src/models/*.ts`).
- **License:** **MIT**.
- **Structure note:** v2 modules are FK-free; cross-module associations are "module links" (link
  tables) and IDs (`region_id`, `variant_id`) are stored as **plain text references**, not FKs.

**Core data-model sketch (by module)**

| Entity | Key fields | Relationships |
|---|---|---|
| `Product` | id (`prod_`), title, handle (unique), status (DRAFT/PROPOSED/PUBLISHED/REJECTED), jsonb metadata | hasMany Variant/Image; belongsTo Type/Collection; M2M Tag/Category |
| `ProductVariant` | sku/barcode/ean/upc (unique), allow_backorder, manage_inventory | belongsTo Product; M2M OptionValue |
| `ProductCategory` | tree (self-ref hierarchy) | taxonomy |
| `Customer` | id (`cus_`), email, has_account | hasMany Address; M2M CustomerGroup |
| `Cart` | id (`cart_`), region_id?, customer_id?, currency_code, ~25 **computed** total fields | hasMany LineItem/ShippingMethod |
| `Order` | id (`order_`), display_id, **version**, status, currency_code, canceled_at | event-sourced |
| `OrderLineItem` | **denormalized snapshot** (product_title, variant_sku), unit_price as **bigNumber** | child tax_lines + adjustments |
| `OrderChange`/`OrderChangeAction`/`Return`/`Exchange`/`Claim` | versioned change-sets over order timeline | — |
| `PriceSet` / `Price` | currency_code, amount (bigNumber), min/max_quantity (tiered) | hasMany PriceRule |
| `PriceList` / `PriceListRule` | status (DRAFT/ACTIVE), type (SALE/OVERRIDE), starts_at/ends_at | contextual/scheduled pricing |
| `InventoryItem` / `InventoryLevel` / `ReservationItem` | sku; location_id, stocked/reserved/available_quantity | decoupled from Product |

**Standout patterns:** modular bounded-context design (cross-module refs are text ids + link
tables); **arbitrary-precision money** (`bigNumber` raw + numeric); computed/derived totals (not
persisted); soft-delete everywhere (`deleted_at` + partial unique indexes `WHERE deleted_at IS
NULL`); state machines (Product/Order/PriceList status); event-sourcing/versioning (`Order.version`
+ OrderChange, Returns/Exchanges/Claims); i18n via `.translatable()` marking; hierarchical
ProductCategory tree; jsonb metadata; denormalized order-line snapshots; rule-engine pricing.

**ForgeDB take:** Best **e-commerce / rich-relational** slot — money types, soft-delete with partial
indexes, computed fields, versioned/state-machine orders, taxonomy trees. Its FK-free modules make
a good case study for ForgeDB `Model` FK vs loose-reference. Distill ~12 tables, not all 21 modules.

---

## 7. Zulip — Team chat (topics/threading)

- **Domain:** Org-scoped team chat organized around threaded topics within streams/channels.
- **Repo:** https://github.com/zulip/zulip (Django; `zerver/models/*.py`).
- **License:** **Apache-2.0** (permissive).

**Core data-model sketch**

| Entity | Key fields | Relationships |
|---|---|---|
| `Realm` | string_id (subdomain), uuid, plan_type, org_type, message_retention_days; many `can_*_group` | tenant root; nearly every table FKs here |
| `UserProfile` | realm, delivery_email/email (unique per realm), role (Owner/Admin/Moderator/Member/Guest), is_active (soft-delete), is_bot, bot_owner→self, uuid | extends abstract UserBaseSettings (~40 prefs) |
| `RealmUserDefault` | 1:1 → Realm | realm-level defaults for new users |
| `Recipient` | **type (STREAM/DIRECT_MESSAGE_GROUP) + type_id**; unique(type, type_id) | hand-rolled generic FK (polymorphic) |
| `Stream` | realm, name, recipient→Recipient, invite_only, is_web_public, deactivated | → Recipient |
| `Subscription` | user_profile, recipient, active, is_muted, color, pin_to_top; unique(user, recipient) | membership for streams AND DM groups |
| `Message` | sender, recipient, realm, **subject (= topic, indexed)**, content, edit_history (json), search_tsvector | GIN full-text |
| `UserMessage` | user_profile, message, **flags (bitfield: read/starred/mentioned…)** | per-recipient inbox; largest table |
| `DirectMessageGroup` | huddle_hash (SHA1 of sorted member ids) | membership via Subscriptions |
| `Reaction` | user_profile, message, emoji_name/code, reaction_type | unique constraint |
| `UserGroup` / `NamedUserGroup` | members M2M, subgroups M2M self, is_system_group, `can_*_group` self-FKs | RBAC primitive |
| `RealmAuditLog` | realm, acting_user, modified_*, event_type (50+ enum), extra_data (JSON old/new) | audit trail |

**Standout patterns:** multi-tenancy (row-level `realm_id` everywhere); polymorphic association
(Recipient generic FK); two-layer RBAC (integer role ladder + group-based `can_*_group` perms with
nesting); soft-delete/deactivation + `scheduled_deletion_date`; dedicated audit trail; archival
mirror tables (`Archived*`); full-text tsvector/GIN + topic-as-denormalized-string; bitfield flags
(UserMessage); settings inheritance (realm defaults → user overrides).

**ForgeDB take:** Meaty **team-chat** slot — multi-tenancy, layered RBAC, real polymorphic
association, audit logging, soft-delete/archival, bitfield/denormalized-search in one model.

---

## 8. Gitea — Git / code hosting + issue tracking

- **Domain:** Self-hosted Git service: code hosting + issues, PRs, orgs, Actions CI (Gogs fork).
- **Repo:** https://github.com/go-gitea/gitea (Go/XORM; `models/` subpackages).
- **License:** **MIT**.

**Core data-model sketch** (every entity has `ID int64 pk`)

| Entity | Key fields | Relationships |
|---|---|---|
| `User` (`user` table) | Name/LowerName (unique), Email, **Type (individual vs organization — same table)**, IsAdmin, Num* counters, *Unix timestamps | STI discriminator |
| `Organization` | Go alias over User; TableName→"user"; Type=organization | orgs ARE users |
| `OrgUser` | UID→User, OrgID→Org, IsPublic | membership |
| `Repository` | OwnerID→User, Name, IsPrivate/IsArchived/IsMirror, IsFork + **ForkID→self**, TemplateID→self, Topics[] | fork/template lineage |
| `Team` | OrgID→Org, AccessMode (perm.AccessMode), IncludesAllRepositories, Visibility | junctions TeamUser/TeamRepo/TeamUnit |
| `Collaboration` | RepoID + UserID (composite unique), Mode (AccessMode) | direct per-repo RBAC grant |
| `Issue` | RepoID + **Index (unique per-repo sequential #)**, PosterID, ContentVersion (optimistic lock), MilestoneID, IsClosed, **IsPull** | issues & PRs share table |
| `PullRequest` | **IssueID→Issue (1:1 extension)**, Head/BaseRepoID, HasMerged, Status (Conflict/Checking/Mergeable/...) | state machine |
| `Comment` | **Type (39-value discriminator)**, PosterID, IssueID + type-dependent nullable FKs (LabelID, MilestoneID, ReviewID, CommitSHA...) | discussion + activity/audit log |
| `Label` / `IssueLabel` | RepoID **OR** OrgID, Color, Exclusive(+Order) | repo/org-scoped; M2M tagging |
| `Milestone` | RepoID, IsClosed, Num* counters, DeadlineUnix | — |
| `Release` | RepoID + TagName (unique), IsDraft/IsPrerelease/IsTag | state flags |
| `Star` / `Watch` | UID + RepoID; Watch Mode (None/Normal/Dont/Auto) | M2M / subscription w/ opt-out enum |

**Standout patterns:** single-table inheritance/discriminator (`User.Type`; `Issue.IsPull` +
`PullRequest` 1:1 satellite); multi-tenancy via orgs + visibility; two-layer RBAC (team AccessMode
+ unit scoping + per-repo Collaboration.Mode; shared enum None→Read→Write→Admin→Owner); polymorphic
event log (`Comment.Type` + conditional FKs); per-tenant sequential IDs (`unique(repo_index)`);
state machines (PR status, Watch mode, IsClosed/HasMerged/IsDraft); self-ref lineage
(ForkID/TemplateID); tagging; denormalized Num* counters; Unix-epoch timestamps (mostly hard
deletes); optimistic concurrency (ContentVersion).

**ForgeDB take:** Relationally rich **git/code-hosting + issue-tracking** slot — STI, two-tier RBAC,
polymorphic event log, per-tenant keys, tagging, self-ref lineage in a clean, normalized MIT reference.

---

## 9. Outline — Team wiki / knowledge base

- **Domain:** Team wiki / knowledge base with real-time collaborative editor; docs in collections.
- **Repo:** https://github.com/outline/outline (Node.js/Sequelize; `server/models/*.ts`).
- **License:** **BSL 1.1** — source-available, NOT OSS. Change Date **2030-06-06 → Apache-2.0**;
  Additional Use Grant forbids running a competing multi-tenant "Document Service" SaaS. **Use as
  a design study only, not redistributable.**

**Core data-model sketch**

| Entity | Key fields | Relationships |
|---|---|---|
| `Team` | subdomain (unique), domain (custom FQDN), defaultCollectionId, defaultUserRole, JSONB preferences/flags, suspendedAt | tenant root |
| `User` | email, name, role ENUM(Admin/Member/Viewer/Guest), teamId, invitedById/suspendedById→self, language/timezone (i18n), **deletedAt (paranoid)** | soft-delete |
| `Collection` | urlId, name, permission?, **documentStructure (JSONB tree cache)**, index (fractional), teamId, archivedAt, deletedAt | M2M User/Group via memberships |
| `Document` | urlId, title, previousTitles[], content (JSONB Prosemirror), **state (BLOB YJS/CRDT)**, version, collaboratorIds[], publishedAt (null=draft), archivedAt, deletedAt, **parentDocumentId→self**, collectionId, teamId | self-ref tree |
| `Revision` | version, title, content (JSONB snapshot), documentId, userId | full snapshot per revision |
| `Comment` | data (JSONB), reactions (JSONB), resolvedAt, documentId, **parentCommentId→self** | threaded |
| `Share` | published, includeChildDocuments, urlId, domain (unique), revokedAt, views, documentId?/collectionId? | publish/revoke |
| `Star` / `View` | index (fractional); count, lastEditingAt | read analytics |
| `Group` / `GroupUser` | name, externalId, teamId; permission enum | aggregate users |
| `UserMembership` | permission (default ReadWrite), index, userId, collectionId **or** documentId, **sourceId→self (perm-inheritance root)** | RBAC junction |
| `GroupMembership` | parallel group↔resource | — |
| `Attachment` | key (storage path), contentType, size, acl, teamId, documentId?, userId | — |

**Standout patterns:** multi-tenancy (hard teamId root; subdomain + custom-domain isolation);
layered RBAC (team role enum + resource ACLs via UserMembership/GroupMembership → collection OR
document; Groups aggregate); permission inheritance (`UserMembership.sourceId` propagates to nested
children); soft-delete (`deletedAt` paranoid) distinct from archive (`archivedAt`); versioning
(Revision snapshots); hierarchical trees (`parentDocumentId` + denormalized `documentStructure`
JSONB + fractional index); collaborative CRDT state (YJS blob); publish/share state machine; rich
audit FKs (createdById/lastModifiedById/archivedById/invitedById/resolvedById); i18n; fractional
indexing for drag-reorder.

**ForgeDB take:** Near-ideal **wiki / knowledge-base / docs** slot — multi-tenancy, self-referential
document trees, versioning, group+membership RBAC with inheritance, soft-delete/archive duality,
publish/share state, collaborative blobs. **License caveat: BSL 1.1 — design inspiration only.**

---

## 10. Documenso — E-signature (open-source DocuSign)

- **Domain:** Digital document signing (e-signature workflows).
- **Repo:** https://github.com/documenso/documenso (Prisma; `packages/prisma/schema.prisma`).
- **License:** **AGPLv3**.

**Core data-model sketch**

| Entity | Key fields | Relationships |
|---|---|---|
| `User` | email (unique), password, roles[], twoFactorEnabled | owns Account/Session/ApiToken/Webhook/Passkey |
| `Organisation` | name, url (unique), type (PERSONAL/ORGANISATION), ownerUserId, organisationClaimId | tenant |
| `OrganisationMember` / `OrganisationGroup(+Member)` | userId, organisationId (unique pair) | group-based access |
| `Team` | name, url, organisationId | owns envelopes/folders/apiTokens/webhooks |
| `Envelope` (central container) | secondaryId, type (DOCUMENT/TEMPLATE), title, status (DRAFT/PENDING/COMPLETED/REJECTED/CANCELLED), visibility, userId, teamId, folderId, documentMetaId, JSON authOptions/formValues | — |
| `EnvelopeItem` | documentDataId, envelopeId, order | files/pages |
| `DocumentData` | **type (S3_PATH/BYTES/BYTES_64)**, data, initialData | polymorphic-ish storage |
| `DocumentMeta` | subject, message, signingOrder (PARALLEL/SEQUENTIAL), distributionMethod, reminder/expiry | — |
| `Recipient` | envelopeId, email, token (unique), role (SIGNER/CC/VIEWER/APPROVER/ASSISTANT), readStatus, signingStatus (NOT_SIGNED/SIGNED/REJECTED), sendStatus, signingOrder | per-recipient state |
| `Field` | envelopeId, envelopeItemId, recipientId, type (SIGNATURE/INITIALS/DATE/TEXT/CHECKBOX/DROPDOWN...), page, positionX/Y, w/h, JSON fieldMeta | — |
| `Signature` | recipientId, fieldId (unique 1:1), signatureImageAsBase64 / typedSignature | — |
| `Folder` | **parentId→self (tree)**, userId, teamId, visibility | — |
| `DocumentAuditLog` / `UserSecurityAuditLog` | envelopeId, type, data (JSON), ipAddress | immutable dual audit trails |
| `Subscription` / `SubscriptionClaim` / `OrganisationClaim` | Stripe billing + quotas (documentQuota, memberCount, rate limits) | entitlements |
| `ApiToken` / `Webhook`(+WebhookCall) / `BackgroundJob`(+Task) / `RateLimit` / `EmailDomain` (DKIM) | — | infra |

**Standout patterns:** multi-tenancy (User→Organisation→Team→Envelope, isolation via `visibility`
enums); RBAC (member roles ADMIN/MANAGER/MEMBER + groups + row visibility); soft-delete (`deletedAt`
+ cascades); versioning (`internalVersion`); dual immutable audit trails; state machines (document,
recipient signing/read/send, subscription, job status); polymorphic file storage via DocumentData
type enum; money/billing (Subscription + quota claims); background-job queue; rich Envelope→Recipient
→Field→Signature 1:many:1 signing graph.

**ForgeDB take:** Meaty **document-workflow / e-signature** slot — nested containers, per-recipient
state machines, audit logging, quota billing; deep FK chains + status enums without EAV complexity.

---

## 11. Twenty — CRM (metadata-driven)

- **Domain:** CRM / Salesforce alternative with a runtime-configurable data model.
- **Repo:** https://github.com/twentyhq/twenty (TypeScript/NestJS/TypeORM;
  `packages/twenty-server/src/engine/metadata-modules/` + standard-object entities).
- **License:** **AGPLv3** default; files marked `/* @license Enterprise */` are proprietary
  (Twenty.com Commercial License).

**Two-layer data model** — the CRM schema is **data, not DDL**; a Postgres metadata layer drives
runtime generation of per-workspace tables + the GraphQL API.

**Metadata layer (EAV / metadata-driven):**

| Entity | Role |
|---|---|
| `Workspace` | tenant root; each workspace gets isolated data tables sharing core metadata |
| `objectMetadata` | nameSingular/namePlural, isCustom, isSystem, workspaceId — **each row defines a table** |
| `fieldMetadata` | objectMetadataId, name, type (UUID/TEXT/DATE_TIME/BOOLEAN/NUMERIC/SELECT/RELATION/TS_VECTOR), isNullable — **each row = one column** |
| `relationMetadata` / `indexMetadata` | link objects; index defs |
| `role` / `objectPermission` / `fieldPermission` / `rowLevelPermissionPredicate` / `rolePermissionFlag` | object + field + row-level RBAC |
| `view` / `viewField` / `viewFilter` / `viewSort` / `pageLayout` / `navigationMenuItem` | UI-as-data |

**Standard business objects (generated records):**

| Entity | Key fields | Relationships |
|---|---|---|
| `Person` | name, email, phone, city, companyId | contact/lead |
| `Company` | name, domainName, employees, address | — |
| `Opportunity` | name, **amount (money micro + currency)**, stage, closeDate, companyId, pointOfContactId | → Company, Person |
| `Note` / `Task` | title, body, dueAt/status | — |
| `NoteTarget` / `TaskTarget` | **polymorphic join** — noteId/taskId + optional personId/companyId/opportunityId/custom | links activities to any object |
| `WorkspaceMember` | userId, name, colorScheme | user within workspace |
| `Attachment` / `Favorite` / `Comment` / `TimelineActivity` / `ApiKey` / `Webhook` / `ConnectedAccount` / `MessageChannel` / `CalendarChannel` | email/calendar sync | — |

**Standout patterns:** EAV / metadata-driven schema (objects & fields are rows, tables generated at
runtime — the headline pattern); multi-tenancy (per-Workspace isolation w/ shared metadata);
fine-grained RBAC (object + field + row-level); polymorphic associations (NoteTarget/TaskTarget fan
out to any object); money (Opportunity.amount micro-amount + currency); audit via TimelineActivity;
UI config as data; full-text via TS_VECTOR field type.

**ForgeDB take:** **CRM + advanced-patterns** slot, deliberately hard. Its runtime-metadata
philosophy is the *opposite* of ForgeDB's compile-time generation — so mine the **standard objects**
(Person/Company/Opportunity/Task/Note + polymorphic targets) as the example schema, NOT the
objectMetadata/fieldMetadata EAV layer.

---

## 12. Formbricks — Surveys / experience management

- **Domain:** Survey & experience-management platform (in-app + link surveys, contact analytics).
- **Repo:** https://github.com/formbricks/formbricks (Prisma; `packages/database/schema.prisma`, pgvector).
- **License:** **AGPLv3** core (mixed — some packages MIT, enterprise edition separate proprietary).

**Core data-model sketch**

| Entity | Key fields | Relationships |
|---|---|---|
| `Organization` | name, JSON whitelabel/billing | → memberships, workspaces, teams, apiKeys |
| `Workspace` (tenant unit under org) | organizationId, recontactDays, JSON styling/config | owns surveys, contacts, actionClasses, segments, tags |
| `User` | email, password, identityProvider, locale, twoFactorEnabled, JSON notificationSettings | — |
| `Membership` | userId, organizationId, role (owner/manager/member/billing), composite PK | — |
| `Team` / `TeamUser` (admin/contributor) / `WorkspaceTeam` (read/readWrite/manage) | group-based access | teams→workspaces mapping |
| `Survey` | name, type (link/app), status (draft/inProgress/paused/completed), workspaceId, createdBy, slug; heavy JSON (questions, blocks[], endings[], variables, styling); display rules | — |
| `SurveyLanguage` / `Language` | languageId, surveyId, default, enabled (composite PK); code, alias, workspaceId | **i18n** |
| `SurveyTrigger` / `SurveyAttributeFilter` | surveyId, actionClassId; attributeKeyId, condition, value | event triggers |
| `Response` | surveyId, contactId, displayId, finished, language, JSON data/variables/meta/contactAttributes | → tags, quotaLinks |
| `Display` | surveyId, contactId | impression tracking (recontact logic) |
| `Contact` | workspaceId | → responses, attributes, displays |
| `ContactAttributeKey` | key, name, type (default/custom), dataType (string/number/date), isUnique, workspaceId | **attribute schema** |
| `ContactAttribute` | contactId, attributeKeyId, **value / valueNumber / valueDate (typed EAV)** | per-type indexes |
| `ActionClass` | name, type (code/noCode), key, workspaceId, JSON noCodeConfig | trackable events |
| `Segment` | title, workspaceId, isPrivate, JSON filters | audience targeting |
| `Tag` / `TagsOnResponses` | M2M response tagging | — |
| `SurveyQuota` / `ResponseQuotaLink` | status (screenedIn/screenedOut) | quota/screening |
| `Webhook` / `Integration` / `ApiKey`(+ApiKeyWorkspace) | source, triggers[]; type (googleSheets/notion/airtable/slack) | — |

**Standout patterns:** nested multi-tenancy (Organization→Workspace→resources); layered RBAC
(OrganizationRole, TeamUserRole, WorkspaceTeamPermission, ApiKeyPermission); **typed EAV**
(ContactAttributeKey/ContactAttribute with string/number/date value columns + per-type indexes);
i18n (Language + per-survey SurveyLanguage); heavy JSON for flexible survey definitions; M2M
tagging; state machines (survey/response/source status); quota/screening state; soft-delete via
`isArchived`; event-trigger automation; pgvector embeddings.

**ForgeDB take:** Strong **surveys / form-builder / event-tracking** slot — best for typed EAV
(contact attributes), i18n, M2M tagging, quota state; middle-complexity, less sprawling than
Documenso's billing layer.

---

## 13. Plausible Analytics — Analytics SaaS (Postgres metadata)

- **Domain:** Privacy-friendly web analytics (cookie-free GA alternative).
- **Repo:** https://github.com/plausible/analytics (Elixir/Ecto; Postgres for app metadata,
  ClickHouse for events).
- **License:** **AGPL-3.0**.

**Core Postgres app-metadata model** (Ecto schemas under `lib/plausible/`)

| Entity | Key fields | Relationships |
|---|---|---|
| `users` (Auth.User) | email, password_hash, theme (enum), email_verified, TOTP (enabled/secret encrypted/token), last_team_identifier | has_many team_memberships, api_keys |
| `teams` (Teams.Team) | identifier (UUID), name, trial_expiry_date, locked, hourly_api_request_limit, embedded policy/grace_period | **tenant root** |
| `sites` (Site) | domain (unique), timezone, public, feature flags, allowed_event_props[], team | has_many goals, guest_memberships |
| `team_memberships` (Teams.Membership) | role enum (owner/admin/editor/viewer/billing/guest), is_autocreated, user + team | team-wide RBAC join |
| `guest_memberships` (Teams.GuestMembership) | role enum (viewer/editor), team_membership + site | **second RBAC tier scoping to one site** |
| `team_invitations` / `guest_invitations` | invitation_id (token), email, role, inviter + team | token invitations |
| `subscriptions` (Billing) | paddle_subscription_id (unique), paddle_plan_id, status enum, next_bill_amount/date, currency_code, team | money + state machine |
| `enterprise_plans` | paddle_plan_id, billing_interval, monthly_pageview_limit, site_limit, team_member_limit, features[] | quota tiers |
| `goals` (Goal) | event_name, page_path, scroll_threshold, currency (revenue goals), custom_props, site | many_to_many funnels |
| `api_keys` | name, scopes[], key_hash, key_prefix, user + team | — |
| `shared_links` / `segments` | slug (unique), password_hash?; saved dashboard filter sets | public/password-protected sharing |
| `weekly_report` / `monthly_report` | one per site | scheduled email configs |

**ClickHouse events (separate columnar, out of relational scope):** `events_v2` (name, site_id,
timestamp, session_id, pathname, country_code, utm_*, nested meta.key/value), `sessions_v2` rollups,
`imported_*` GA pre-aggregates.

**Standout patterns:** multi-tenancy (Team as boundary); **two-tier RBAC** (team-wide role enum +
per-site guest override — genuine hierarchical authorization); token-based invitations; subscription
status state machine + money (next_bill_amount, currency_code) + multi-tier quota limits;
embedded/value objects (Policy, GracePeriod via Ecto embeds); secrets-at-rest (encrypted TOTP,
hashed API keys/passwords). Not present: soft-delete, i18n tables, DB-level RLS (app-enforced).

**ForgeDB take:** Realistic **B2B SaaS multi-tenant + metered billing** slot — tenant scoping
(team→sites), FK-heavy RBAC join tables with role enums, token invitations, subscription/plan quota
modeling. ClickHouse side out of scope for a relational app-DB example.

---

## 14. Saleor — E-commerce (multi-channel, GraphQL)

- **Domain:** Headless API-first e-commerce for stores, marketplaces, multi-channel retail.
- **Repo:** https://github.com/saleor/saleor (Python/Django; `saleor/*/models.py`).
- **License:** **BSD-3-Clause** (permissive).

**Core data-model sketch**

| Entity | Key fields | Relationships |
|---|---|---|
| `User` (account) | email (unique), is_staff, uuid, number_of_orders (denorm), metadata/private_metadata | FK default addresses; M2M addresses/groups/permissions |
| `Group` (account) | name; M2M permissions; **M2M channels + restricted_access_to_channels** | channel-scoped RBAC |
| `Channel` | name, slug (unique), currency_code, default_country, strategy fields | **central tenancy dimension** |
| `Category` (product) | name, slug, description (JSON), **self-FK parent (MPTT tree)**, SEO | hierarchical |
| `ProductType` | kind, has_variants, is_shipping_required, is_digital; FK tax_class | — |
| `Product` | name, slug, description (JSON), rating, search_vector; FK type/category, O2O default_variant | — |
| `ProductVariant` | sku, track_inventory, preorder, quantity_limit_per_customer; FK product | M2M media |
| `ProductChannelListing` / `ProductVariantChannelListing` | FK product/variant + channel; visible_in_listings, available_for_purchase_at, price_amount, cost_price, discounted_price | **per-channel availability/pricing** |
| `Attribute` / `AttributeValue` | slug, input_type, entity_type; **polymorphic value storage** (value/rich_text/boolean/date_time/numeric + reference FKs) | EAV via AssignedProductAttribute* |
| `Order` | UUID, number (unique), status (DRAFT/UNFULFILLED/.../CANCELED/EXPIRED), authorize_status, charge_status; FK user/channel/addresses/voucher; net/gross/charged money | state machine |
| `OrderLine` | FK order/variant/tax_class; quantity_fulfilled, unit/total net+gross, unit_discount, is_gift | — |
| `Fulfillment` / `FulfillmentLine` | status (FULFILLED/REFUNDED/RETURNED), tracking_number; FK order_line/stock | — |
| `OrderEvent` (+ CustomerEvent/PromotionEvent/TransactionEvent) | type, parameters (JSON) | audit trail |
| `Checkout` / `CheckoutLine` | UUID token pk; FK user/channel/addresses, voucher_code, TaxedMoney totals | M2M gift_cards |
| `Warehouse` / `Stock` / `Allocation` / `Reservation` | click_and_collect, is_private; quantity/quantity_allocated (unique warehouse+variant); reserved_until | inventory |
| `Voucher` / `VoucherCode` / `Promotion` / `PromotionRule` | type, discount_value_type; catalogue/order_predicate (JSON), reward_value | rule-engine discounts |
| `Payment` / `Transaction` / `TransactionItem` / `TransactionEvent` | gateway, charge_status, captured_amount; kind (AUTH/CAPTURE/VOID/REFUND), idempotency_key | — |
| `*Translation` tables | keyed by language_code | **i18n** |

**Standout patterns:** multi-tenancy via **Channels** with per-channel listing tables decoupling
availability/pricing; RBAC (Django PermissionsMixin + Group↔Permission with Group→Channel scoping);
near-universal metadata/private_metadata JSON + dedicated `*Event` audit tables; **MPTT hierarchical
Category tree**; **EAV/tagging** via Attribute/AttributeValue (polymorphic value columns +
reference-typed values); state machines (Order/Fulfillment status, authorize/charge_status);
pervasive money + currency (net+gross TaxedMoney, tax-inclusive); **i18n via dedicated `*Translation`
tables**; denormalization/full-text; external-reference integration fields.

**ForgeDB take:** High-realism **e-commerce / multi-channel retail** slot — channels, per-channel
listings, EAV attributes, MPTT categories, money+tax, order/fulfillment state machines, inventory
allocation/reservation. Exercises FKs, M2M-through tables, JSON, enums, self-ref trees far beyond a toy.

---

## 15. Supabase Slack-clone — Auth + RLS + RBAC pattern

- **Domain:** Realtime Slack-style chat demonstrating Postgres Row-Level Security + JWT-claim RBAC.
- **Repo:** https://github.com/supabase/supabase/tree/master/examples/slack-clone/nextjs-slack-clone
  (`supabase/migrations/*_init.sql`, `*_auth-hook.sql`). Docs: Custom Claims & RBAC guide.
- **License:** **Apache-2.0** (repo-level).

**Core data-model sketch**

Enums: `app_role` (admin, moderator); `app_permission` (channels.delete, messages.delete);
`user_status` (ONLINE, OFFLINE).

| Entity | Key fields | Relationships |
|---|---|---|
| `users` (profile) | **id uuid PK → auth.users (1:1 mirror)**, username, status | — |
| `channels` | id bigint, slug (unique), inserted_at; created_by → users | — |
| `messages` | id bigint, message text; user_id → users, channel_id → channels (ON DELETE CASCADE) | — |
| `user_roles` | role app_role, unique(user_id, role); user_id → users | RBAC |
| `role_permissions` | role app_role, permission app_permission, unique(role, permission) | role→permission map (no FK) |

**Standout patterns (RLS + RBAC):**
1. **auth.users mirroring via trigger** — `public.users.id` PK/FK on `auth.users`; SECURITY DEFINER
   trigger `handle_new_user()` copies new auth user in + seeds a `user_roles` row.
2. **Owner-scoped RLS via `auth.uid()`** — `messages` insert `with check (auth.uid() = user_id)`,
   update/delete `using (auth.uid() = user_id)`; broad reads via `auth.role() = 'authenticated'`.
3. **Permission check via SECURITY DEFINER `authorize()`** — counts `role_permissions` matching the
   requested permission + caller's role from JWT claim; privileged deletes layer a second policy so
   owners OR permitted roles can delete.
4. **Custom access-token hook** injects the role claim into the JWT at mint time.

**ForgeDB take:** Compact **multi-tenant / SaaS auth + RLS** slot — complete authorization model
(owner-scoped policies + enum RBAC + JWT-claim `authorize()` + auth.users mirroring); ideal target
for schema-to-code generation of access-control-aware DB code.

---

## Cross-cutting pattern index

| Pattern | Best examples |
|---|---|
| Multi-tenancy (row-level tenant_id) | Chatwoot, Zulip, Plausible, Cal.com, Formbricks, Outline |
| Hierarchical org/team (self-ref) | Cal.com (Team.parentId), Gitea (orgs=users), Formbricks/Documenso (org→team→ws) |
| RBAC (roles + permissions) | Ghost (full matrix), Zulip/Gitea (two-layer), Twenty (object/field/row), Plausible (two-tier) |
| Row-Level Security (RLS) | Supabase Slack-clone |
| Soft-delete + archive | Discourse, Mastodon, Medusa (partial indexes), Outline (deletedAt vs archivedAt), Zulip |
| Audit trail / event log | Cal.com, Chatwoot, Zulip (RealmAuditLog), Gitea (Comment.Type), Documenso (dual), Saleor (*Event) |
| Polymorphic associations | Mastodon, Zulip (Recipient), Chatwoot (channels/sender/taggings), Twenty (NoteTarget), Gitea (Comment) |
| Hierarchical/tree data | Discourse (categories), Medusa/Saleor (category MPTT), Outline (documents), Documenso (folders) |
| Tagging (M2M) | Discourse, Mastodon, Chatwoot, Gitea (labels), Formbricks |
| Versioning | Discourse/Ghost (revisions), Outline (Revision snapshots), Medusa (Order.version), Gitea (ContentVersion) |
| State machines | Ghost, Cal.com (Booking), Medusa/Saleor (Order/Fulfillment), Documenso (recipient), Gitea (PR) |
| Money / currency | Ghost, Cal.com, Medusa (bigNumber), Saleor (net+gross TaxedMoney), Twenty, Plausible |
| i18n | Cal.com (translation table), Formbricks (SurveyLanguage), Saleor (*Translation), Medusa (translatable) |
| EAV / metadata-driven | Twenty (objectMetadata/fieldMetadata), Formbricks (typed ContactAttribute), Saleor (Attribute), Chatwoot |
| Denormalized counters | Discourse, Gitea (Num*), Mastodon, Saleor |
| Bitfield flags | Zulip (UserMessage.flags), Mastodon (user_roles permissions) |
| Federation / local-vs-remote | Mastodon (domain/uri) |

## License summary

| License | Apps |
|---|---|
| MIT (safest) | Ghost, Chatwoot (core), Medusa, Gitea |
| BSD-3-Clause | Saleor |
| Apache-2.0 | Zulip, Supabase example |
| AGPL-3.0 | Mastodon, Cal.com (core), Documenso, Twenty (core), Formbricks (core), Plausible |
| GPL-2.0 | Discourse |
| BSL 1.1 (source-available, NOT OSS) | Outline — design study only |

All entries used strictly as inspiration; no verbatim structure copied.
