# SPIKE #101 — composite `@index(a,b)` indexes + multi-field probes

Design spike for issue #101, a Phase 2 (#90) follow-up. Builds on the single-field secondary-index
machinery landed in #90 (`crates/codegen/src/rust.rs`). Verified against generated code + the
parser AST as of 2026-07-10.

## TL;DR recommendation

**Generate one composite index per `@index(a,b,…)` keyed by the ordered concatenation of each
component field's #90 canonical string key, joined by a byte that cannot occur in the parts.**
Emit a `find_by_<a>_and_<b>(a, b, …)` probe (+ snapshot `_at`). Reuse #90's `index_add_block` /
`index_remove_block` / `index_key_expr` per component and combine — the maintenance sites and
reopen rebuild extend by iterating `model.composite_indexes` in addition to `indexed_fields`.
**Do not** try to make a composite index serve arbitrary single-field prefixes in v1 (that's a
B-tree-ordering feature; our index is a hash map).

## The deciding insight

#90's index is a **hash map keyed by a canonical string**. A composite index over `(a, b)` is the
*same* hash map keyed by a **tuple's** canonical string — `key(a) ⧉ key(b)`. Everything else (the
`HashSet<Id>` value, maintenance timing, snapshot `_at` resolution, reopen rebuild) is identical
to a single-field index. The only two real design questions are:

1. **Key encoding** — how to combine per-field keys so `(a="x|y", b="z")` and `(a="x", b="y|z")`
   never collide.
2. **Where the index list comes from** — single-field indexes come from `indexed_fields`
   (per-`Field`); composites come from `Model.composite_indexes` (per-`CompositeIndex { fields:
   Vec<String> }`), a different iteration source.

Both are small. This is a hash-map key-shaping problem, not a new index kind.

## Design

### 1. Key encoding (collision-free)

Per-component key uses #90's `index_key_expr` unchanged (JSON canonical string). Combine with a
**length-prefixed** or **control-char-delimited** join. Recommend length-prefix (robust; no
"forbidden byte" assumption):

```rust
// composite key = concat, each part prefixed by its byte length as decimal + ':'
// e.g. a="pending", b="1699999999"  ->  "7:pending10:1699999999"
fn composite_key(parts: &[String]) -> String {
    let mut s = String::new();
    for p in parts { s.push_str(&p.len().to_string()); s.push(':'); s.push_str(p); }
    s
}
```

This makes `("ab","c")` = `"2:ab1:c"` and `("a","bc")` = `"1:a2:bc"` distinct. (A simple `'\u{1}'`
separator also works since JSON canonical strings for scalars never contain control chars, but
length-prefix is assumption-free and cheap.)

### 2. Index field + selection

Name the field by the component set: `<a>_<b>_index` (e.g. `status_created_at_index`). Source it
from `model.composite_indexes`, validating each component resolves to an indexable scalar field
(reuse `is_filterable_scalar`; error/skip on relation/unknown/nullable — nullable rides #102):

```rust
fn composite_indexes(model) -> Vec<(Ident /*field*/, Vec<&Field> /*components*/)> { … }
```

Struct field type is `HashMap<String, HashSet<Id>>`, identical to single-field.

### 3. Maintenance

At each maintenance site (insert/update/delete) and the reopen rebuild, in addition to the
per-field `indexed_fields` loop, iterate composites: build `composite_key([key(rec.a), key(rec.b)])`
and add/remove the id. On `update`, the old composite key is built from `__old` (already fetched by
#90 when `indexed_fields` is non-empty — extend the "fetch `__old`" trigger to also fire when
composites exist), the new from `record`. Same `index_add_block` / `index_remove_block` shape,
just with the composite key expression substituted for the single-field one.

### 4. Probe

```rust
pub fn find_by_status_and_created_at(&self, status: &str, created_at: Timestamp) -> Vec<Order> {
    let __k = composite_key(&[key(status), key(created_at)]);
    match self.status_created_at_index.get(&__k) { … resolve via get, like #90 … }
}
// + find_by_status_and_created_at_at(&Snapshot, …) resolving via get_at + post-filter
```

Param types follow #90's `index_param_type` per component (`&str` for string, Copy type otherwise).
Uniqueness: if all components are `&`-unique together (rare; composite-unique isn't in the grammar
today) we could emit `get_by_…`; for v1, **Vec probe only**.

### 5. (Optional) list-endpoint use

When a REST list filter names *exactly* a composite's component set, the handler could route to
the composite probe instead of `all()` + scan-filter — a pure perf optimization. **Not required
for correctness** (the #90 list path already works via the closed-set filter). Defer; note it.

## Constraints (inherited)

- Commit boundary / superseding-version (#89/#66): same sites, `__old`-based removal. ✅ (with the
  `__old`-fetch trigger extended to composites).
- Snapshot isolation (#56): composite `_at` resolves via `get_at` + post-filter (rebuild both
  component keys from the resolved record and compare). ✅
- Reopen: composite rebuilt in the same id-scan. ✅
- Identity: generated, schema-tailored; the "predicate" is a fixed component list from the schema,
  not a runtime expression. ✅

## Tradeoffs / decisions

- **Hash, not B-tree.** Our index is exact-match only, so a composite `(a,b)` index answers
  `a=? AND b=?` but **not** `a=?` alone (no prefix scan) nor range `b > ?`. That matches #90's
  single-field hash semantics. Prefix/range would need an ordered index — a separate, larger
  design (call it out as explicitly out of scope; it's a Phase 4+/perf concern).
- **Component validation.** `@index(a,b)` where a component is a relation, unknown field, or
  nullable → validation error at generate time (or skip-with-warning). Prefer a hard validation
  error in `crates/validation` so the schema author learns early; nullable components unblock via
  #102.
- **Key cost.** One extra `HashMap` entry per row per composite index. Bounded and opt-in
  (only declared composites).

## Recommended plan

1. Add `composite_indexes(model)` selector + validation of components (indexable scalars).
2. Emit composite index fields + inits; extend maintenance sites + reopen rebuild to iterate them
   (extend the `__old`-fetch trigger).
3. Emit `find_by_<components>` (+ `_at`) probes with the length-prefixed composite key.
4. Guard: `@index(status, created_at)` on `ecommerce-store`'s `Order` generates the field,
   maintenance, and a `find_by_status_and_created_at` that hits the map (not a scan).
5. E2E: insert Orders, probe by `(status, created_at)`, snapshot isolation; assert collision-free
   key (two rows whose concatenations would collide under naive join resolve distinctly).
6. (Optional) route matching list filters to the composite probe.

**Scope:** 1 codegen file, validation touch-up, 1 guard, 1 E2E. No new substrate, no publish gap.
