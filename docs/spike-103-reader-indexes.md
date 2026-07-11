# SPIKE #103 — secondary-index probes on `DatabaseReader` (concurrent reads)

Design spike for issue #103, a Phase 2 (#90) follow-up. Builds on the #90 index machinery and the
#56 Direction-B reader handles (`crates/codegen/src/rust.rs`). Verified against generated code as
of 2026-07-10.

## TL;DR recommendation

**Clone the writer's `value→{id}` indexes into the reader handle (like `id_to_row` is already
cloned in `generate_reader_inits`) and generate `find_by_<field>_at` / `get_by_<field>_at` on
`*StorageReader`, resolving through the reader's existing `get_at`.** The reader is snapshot-only
(no live `get`), so it gets **only the `_at` probes** — which is exactly right. This is a
mechanical mirror of #90's writer probes onto the reader, with one honest, documented caveat: the
cloned index is a **point-in-time snapshot of the live index**, captured when `reader()` is called.

## The deciding insight

The reader already establishes the precedent this spike needs. `generate_reader_inits` does
`id_to_row: self.id_to_row.clone()` and the reader reuses the **exact same** `read_at` / `get_at` /
`all_at` token streams as the writer (one decode path, no drift). A secondary index is *the same
kind of derived in-memory state as `id_to_row`* — so it clones the same way, and the `_at` probe is
the **same token stream** #90 already emits, just reading `self.<field>_index` on the reader struct
instead of the writer.

The reader has **no live `get`** (it reads a snapshot, not the mutable newest state), so the
writer's live `find_by_<field>` / `get_by_<field>` have no reader analogue — only the snapshot
`_at` forms. That's a feature: it removes the temptation to expose a "live" probe on a handle whose
whole purpose is consistent point-in-time reads.

## Design

### 1. Reader struct gains the index fields

`generate_reader_storage_fields` currently emits `id_to_row` + reader columns + tombstones. Add,
for each `indexed_fields` entry, the same `HashMap<String, HashSet<Id>>` field:

```rust
#index_ident: std::collections::HashMap<String, std::collections::HashSet<#id_type>>,
```

### 2. Reader inits clone the index

`generate_reader_inits` currently does `id_to_row: self.id_to_row.clone()`. Add per index:

```rust
#index_ident: self.#index_ident.clone(),
```

This clone captures the index **as of `reader()`** — the same instant the reader's column readers
are opened. Pairing the reader with a `DatabaseSnapshot` captured on the writer (the #56 discipline)
gives a coherent view: the snapshot watermark and the cloned index are both taken on the single
writer, off the mutation path.

### 3. Generate `_at` probes on the reader

Factor #90's probe emission so the `_at` methods can be emitted against either receiver. The
reader's `get_at` already exists (same token stream as the writer's), so:

```rust
// on *StorageReader
pub fn find_by_<field>_at(&self, snap: &Snapshot, value: <param>) -> Vec<Model> {
    let __k = index_key_expr(value);
    match self.<field>_index.get(&__k) { Some(ids) =>
        ids.iter().filter_map(|&id| self.get_at(snap, id))
           .filter(|r| index_key_expr(r.<field>) == __k)   // post-filter (same as #90)
           .collect(),
        None => Vec::new() }
}
// + get_by_<field>_at for unique fields
```

Refactor note: extract a helper in `generate_index_lookups` that takes the receiver + whether a
live `get` is available, so the writer emits {live + `_at`} and the reader emits {`_at` only} from
one source — no second probe body to drift.

## Constraints

- **Consistency of the clone.** The index clone must be captured on the **single writer**, off the
  mutation path — same rule as `DatabaseSnapshot::snapshot()` and `id_to_row.clone()`. `Database::
  reader()` already runs on the writer, so this holds by construction. Document that a reader's
  index reflects the writer's state at `reader()` time.
- **Snapshot isolation (#56):** `_at` resolves via the reader's `get_at` + post-filter — identical
  correctness to the writer `_at`. The honest limit is identical too: a row whose indexed value
  changed *away* between the reader-capture and the query is not in the cloned index (indexes are
  not versioned). Document once, shared with #90.
- **Identity:** the reader knows *less* than the writer (no maintenance, no live probe) — strictly
  in-bounds; a reader handle is category-1 read glue over generated storage.

## Tradeoffs / decisions

- **Clone cost.** Each `reader()` clones every index (`O(rows)` per index). Readers are opened per
  reader-thread, not per query, so this is amortized — the same order as the already-accepted
  `id_to_row.clone()`. If it ever matters, an `Arc<HashMap>` shared-immutable snapshot is the
  upgrade path (writer swaps an `Arc` on mutation; readers hold the old `Arc`). **Recommend plain
  clone for v1** (matches `id_to_row`, simplest, correct); note `Arc` as the perf escape hatch.
- **Live probe on the reader — intentionally absent.** Don't add `find_by_<field>` (no `_at`) to
  the reader; it has no live `get` and its contract is point-in-time. Keeping only `_at` prevents a
  "which state does this read?" ambiguity.
- **Sequencing.** Purely additive to the reader; depends on nothing but #90. Can land independently
  of #100/#101/#102 (though it should pick up whatever fields those make indexable, since it clones
  `indexed_fields` generically).

## Recommended plan

1. Extend `generate_reader_storage_fields` + `generate_reader_inits` with the index fields/clones.
2. Refactor `generate_index_lookups` to emit `_at` probes against a parameterized receiver; emit
   them on `*StorageReader` (no live probes).
3. Guard: `*StorageReader` has `find_by_<field>_at` and clones `<field>_index`; the writer keeps
   both live + `_at`.
4. E2E: extend the Direction-B concurrent-reader harness (`scratchpad/directionb_compile`) — a
   reader probes by an indexed field under its snapshot while the writer keeps appending; assert
   probe result matches the snapshot, not the live newest row.

**Scope:** 1 codegen file (reader-fields + inits + a probe-emission refactor), 1 guard, 1 E2E
extension. No new substrate, no publish gap (indexes are in-memory; the reader shares fds as today).
