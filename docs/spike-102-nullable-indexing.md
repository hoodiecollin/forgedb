# SPIKE #102 — index nullable scalar fields

Design spike for issue #102, a Phase 2 (#90) follow-up. Builds on the single-field secondary-index
machinery landed in #90 (`crates/codegen/src/rust.rs`). Verified against generated code as of
2026-07-10.

## TL;DR recommendation

**Drop the `!nullable` exclusion in `indexed_fields`, make the probe param `Option<&T>` /
`Option<T>`, AND give the `None` key a distinct sentinel** (a presence tag) because #90's raw-string
key arm collides `None` with a literal `Some("null")`. Mirror the presence-tag encoding #90 already
uses for the nullable-*string column* (`0x00` = None, `0x01` = Some): key `None` → `"\u{0}"`,
`Some(v)` → `"\u{1}" + <#90 canonical key of v>`. Recommend `Option<T>` params plus a
`find_by_<field>_null()` convenience.

## The deciding insight

#90 deliberately skipped nullable fields, and a naive un-skip is **subtly incorrect** — this spike
exists to get the key encoding right. What works vs. what breaks:

- The struct field for a nullable scalar is `Option<T>`. Maintenance (`index_add_block` /
  `index_remove_block`) and reopen rebuild read it via `index_key_expr` and would work on `Option<T>`
  unchanged **at the plumbing level**.
- **But the key COLLIDES.** `index_key_expr`'s String arm returns the *raw* string
  (`Value::String(s) => s`), while `None` → `Value::Null` → the other arm → `.to_string()` →
  `"null"`. So `Some("null")` → `"null"` **and** `None` → `"null"` — verified colliding
  (`scratchpad` check, 2026-07-10). A probe for un-set rows would wrongly return rows whose value is
  the literal string `"null"`, and vice-versa.
- This collision **cannot occur in #90 today**: a single field's index only ever sees one JSON type,
  and non-nullable fields never produce `Value::Null`. It is purely a nullable-field hazard — which
  is exactly why #90 gated nullable out rather than shipping the bug.

So the real design decision is **null-distinct key encoding**, plus the probe-parameter ergonomics.

## Design

### 1. Admit nullable scalars + null-distinct key

`indexed_fields`: remove `&& !matches!(f.field_type, FieldType::Nullable(_))`. Keep the
`is_filterable_scalar` gate (which already recurses through `Nullable`).

Introduce a **presence-tagged key** for nullable fields so `None` can never collide with a real
value (mirrors #90's nullable-*string column* encoding, which already prefixes `0x00`/`0x01`):

```rust
// index_key_expr gains a nullable form (value_expr is Option<T>-typed):
match &(#value_expr) {
    Some(__v) => { let mut k = String::from('\u{1}'); k.push_str(&{ <#90 key of __v> }); k }
    None      => String::from('\u{0}'),
}
```

Both record-side (`index_add_block`/`index_remove_block` read the `Option<T>` field) and param-side
(the probe's `Option<T>` arg) run the **same** nullable key expression, so they stay consistent.
Maintenance sites and the reopen rebuild need no *structural* change — they iterate `indexed_fields`
and call the (now nullable-aware) key helper. Note: the index key is internal to the probes and is
**not** shared with the REST list filter (`<model>_event_matches` compares raw params separately),
so changing the nullable key encoding is safe and local.

### 2. Probe parameter

Two viable ergonomics:

| Option | `find_by_<field>` signature | "match NULL" |
|---|---|---|
| **A. `Option<T>` param** (recommended) | `find_by_email(&self, value: Option<&str>)` | `find_by_email(None)` |
| B. non-null param + `_null` method | `find_by_email(&self, value: &str)` + `find_by_email_null(&self)` | dedicated method |

Recommend **A**: one probe covers both cases, the key is the **presence-tagged nullable key** from
§1 applied to `value: Option<&str>` (record-side and param-side identical), and it reads naturally
(`find_by_email(Some("a@x"))` / `find_by_email(None)`). Optionally *also* emit the `_null()`
convenience for the common query. `index_param_type` gains a nullable arm:

```rust
fn index_param_type(field) -> TokenStream {
    match field.field_type {
        Nullable(inner) if is_string(inner) => quote!{ Option<&str> },
        Nullable(inner)                     => { let t = map(inner); quote!{ Option<#t> } },
        _ /* existing #90 arms */           => …,
    }
}
```

`generate_index_lookups` is otherwise unchanged in shape: `let __k = <nullable key of value>`
uses the §1 presence-tagged form (not the raw serde key), keeping record-side and param-side
identical.

### 3. Snapshot `_at` + post-filter

`find_by_<field>_at` already post-filters by rebuilding the key from the resolved record's field
(`index_key_expr(__rec.field)`), which is `Option`-typed — no change. `None`-bucket probes are
snapshot-safe the same way.

## Constraints (inherited, already satisfied)

- Commit boundary / superseding-version (#89/#66): maintenance at the same sites; an update
  `Some(x) → None` drops the `"x"` bucket and adds the `"null"` bucket. ✅
- Snapshot isolation (#56): `_at` resolves via `get_at` + post-filter. ✅
- Reopen: rebuilt in the id-scan. ✅
- Identity: generated code. ✅

## Tradeoffs / decisions

- **`Option<T>` param vs overloads.** Rust has no overloading; `Option<T>` is the idiomatic
  single-signature answer and keeps `get_by_<unique>` returning `Option<Model>` unambiguous.
- **The `None` bucket can be large.** If most rows have `field = None`, its `HashSet` is big and a
  `find_by_field(None)` returns many rows — but that's a correct answer to a legitimate query, and
  it's still an O(1) bucket lookup + resolve, strictly better than the `all()` scan it replaces.
- **`"null"` collision — REAL, must be encoded around.** Under #90's key arm, `None` → `"null"`
  and `Some("null")` → `"null"` **collide** (verified 2026-07-10). The presence-tag (`\u{0}` / `\u{1}`
  prefix) fixes it: `None` → `"\u{0}"`, `Some("null")` → `"\u{1}null"`. Add a test asserting these two
  do **not** collide. This is the load-bearing correctness item of the whole issue.
- **Interaction with #100 (optional FK).** `?Model` FK → `Option<Uuid>` is a nullable scalar in
  this exact sense. Once #102 lands, #100's optional-FK case is a direct application (index the
  optional FK, `None` = "unlinked"). Sequence: #102 before optional-FK in #100.

## Recommended plan

1. Remove the `!nullable` filter in `indexed_fields`.
2. Add the nullable arm to `index_param_type`; optionally emit `find_by_<field>_null()`.
3. Guard: a schema with `handle: ^string?` generates `handle_index` + `find_by_handle(Option<&str>)`;
   assert the `"null"` vs `"\"null\""` non-collision.
4. E2E: rows with `Some`/`None`, probe both buckets, snapshot isolation across a `Some→None` update.
5. Note in #100 that optional-FK indexing now unblocks.

**Scope:** small — 1 codegen file (mostly deletions + one `index_param_type` arm), 1 guard, 1 E2E.
No new substrate, no publish gap.
