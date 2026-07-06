# Proposal: API-Breaking & Storage Internals — S9, T3, S4, S5

**Status:** APPROVED 2026-07-06 — converted to impl tasks #26 (S9), #27 (S4), #28 (S5), #29 (T3), #30 (retarget codegen). Decision: FIX IN-TREE now, DEFER the crates.io publish — do not bump versions or publish yet; a later pass ships types 0.2.0 + storage 0.1.2. wal untouched.
**Triage task:** #23
**Date:** 2026-07-06

## Summary
Only **one** of the four items is a genuine semver break: **T3** (adding `Value::U32`/`Value::U64`
variants to the published `types` crate). **S9** (read methods to `&self`) is a *relaxation* of
receiver mutability — source-compatible for every existing caller — so it does **not** force a major
bump. **S4** (drop per-append fsync, add explicit `flush()`) and **S5** (type-aware file naming) can
both be done additively. Recommendation: cut **`types` 0.2.0** for T3 (batching any other pending
`types` breaks), and ship **`storage` 0.1.2** (minor) for S9 + S4 + S5 done additively. `wal` does
not change and stays 0.1.1. Version lines remain independent — nothing is normalized.

## Decision table
| Item | Location | API-breaking? | On-disk break? | Decision | One-line rationale |
|------|----------|---------------|----------------|----------|--------------------|
| S9 | `crates/storage/src/lib.rs` reads | No (relaxation `&mut`→`&`) | No | **Do** via positional I/O (`read_at`/`seek_read`) | Removes the `get(&mut self)` wart; compatible for callers |
| T3 | `crates/types/src/lib.rs` `Value` | **Yes** (new enum variants) | No | **Do**, gated into `types` 0.2.0 | u64 currently lossy/misrepresented; only matcher is internal |
| S4 | `crates/storage/src/lib.rs` appends | No (additive `flush()`) | No | **Do** — drop per-append fsync, WAL is durability boundary | 1 fsync per column per row is the dominant write cost |
| S5 | `crates/storage/src/lib.rs` path helpers | No (add new helpers, deprecate old) | No (generated paths already type-aware) | **Do, low priority** | Helpers are unused by generated code; mostly cosmetic |

## Per-item analysis

### S9 — storage reads `&mut self`
- **Current state:** Every reader takes `&mut self` because it does `self.file.seek(SeekFrom::Start(..))?`
  then `self.file.read_exact(..)?` on an owned `std::fs::File` whose OS cursor is shared mutable state
  (`crates/storage/src/lib.rs:234-248` `read_u32`, `:258-272` `read_u64`, `:435-449` `read_bytes`,
  `VariableColumn::read_string` `:525-551`, `Tombstones::is_deleted` `:596-608`). There is **no mmap
  and no cursor field** — the module doc's "memory mapping" is aspirational; it is plain seek-based
  `File` I/O. The `&mut` is forced only by the seek-then-read pattern against a single shared cursor.
- **Break characterization:** The generated `get()` inherits `&mut self`
  (`crates/codegen/src/rust.rs:136-140`, with an explicit apology comment) — a read that demands an
  exclusive borrow, blocking concurrent reads and forcing callers to hold `&mut` on the storage
  struct. **No external crate matches on or wraps these methods**; the only consumers are generated
  code, the storage tests (`crates/storage/tests/lib_tests.rs`, ~93 read/append/is_deleted call
  sites) and examples. Crucially, relaxing an inherent method from `&mut self` to `&self` is
  **source-compatible**: existing callers holding a `&mut` can still call an `&self` method, and
  tests keep compiling unchanged. So despite the task framing, S9 is **not** a semver-major event.
- **Options:**
  - *Keep frozen* — leaves the ergonomic wart and blocks the read-concurrency story. Rejected.
  - *Interior mutability (`RefCell`/`Mutex` around the cursor)* — achieves `&self` but adds runtime
    borrow/lock cost and a panic/poison surface for a pure read. Rejected as heavier than needed.
  - *Positional I/O redesign* — replace seek-then-read with `FileExt::read_at` (Unix `pread`) /
    `seek_read` (Windows), which take `&File` and never touch the shared cursor. Methods become
    `&self` with **zero** added synchronization, and reads become naturally concurrent-safe.
    Preferred.
- **Decision:** **Do it** via positional I/O. Change `read_*`, `read_string`, `read_bytes`,
  `is_deleted`, and `len`/`is_empty` (already `&self`) to `&self`.
- **Rationale:** Fixes the single ugliest ergonomic wart in generated code, unlocks concurrent reads,
  and — because it is a relaxation — costs no downstream break. High value, low risk.
- **Scope if actioned:** 1 file (`crates/storage/src/lib.rs`) — rewrite ~7 reader bodies to
  positional I/O; 1 file (`crates/codegen/src/rust.rs`) — flip generated `get()` to `&self`, drop the
  apology comment; re-accept codegen snapshots (5 `.snap` files); storage tests need no signature
  changes but the throwaway compile-check of generated Rust must be rerun.

### T3 — add `Value::U32` / `Value::U64` variants
- **Current state:** `crates/types/src/lib.rs:207-224` `Value` has `I32, I64, F64, Bool, String,
  Uuid, Timestamp` — **no unsigned variants**. Schema `u32`/`u64` fields have no faithful `Value`
  representation; anything routing an unsigned column value through `Value` must widen into `I64`
  (lossy for `u64 > i64::MAX`) or misuse `I32`. Note the storage `ColumnType` enum **already** has
  `U32`/`U64` (`:172-185`) and `FixedColumn` already has `append_u32`/`read_u32` (`:226-248`), so the
  gap is isolated to the `types::Value` runtime enum.
- **Break characterization:** Adding variants to a public non-`#[non_exhaustive]` enum is a semver-
  major change because a downstream exhaustive `match` would stop compiling. Blast radius in this
  workspace is **tiny**: the only exhaustive matcher is internal — `Value::type_name`
  (`crates/types/src/lib.rs:239-247`); `is_numeric`/`is_string` use non-exhaustive `matches!`.
  `crates/wal` uses a **separate** `WalValue` enum (`crates/wal/src/entry.rs`) and
  `crates/query-params` uses its own `FilterValue` — neither is affected. **Codegen does not
  construct `Value`** (`grep 'Value::' crates/codegen/src/rust.rs` is empty; generated model structs
  use native `u32`/`u64`), so there is effectively no generated-code coupling to retarget. The break
  is real in semver terms but nearly free in practice.
- **Options:**
  - *Add `U32(u32)` + `U64(u64)` variants (plain major bump)* — cleanest; update `type_name`, add
    `From<u32>`/`From<u64>`, extend `is_numeric`. Preferred.
  - *Add variants **and** `#[non_exhaustive]`* — future-proofs against further variant additions, but
    marking the enum `#[non_exhaustive]` now is itself the breaking change we are already paying, and
    it burdens every downstream `match` with a wildcard arm. Optional add-on; defer unless we expect
    more variants soon.
  - *Keep frozen* — leaves `u64` unrepresentable/lossy. Rejected.
- **Decision:** **Add `Value::U32(u32)` and `Value::U64(u64)`**, shipped in `types` 0.2.0. Hold the
  `#[non_exhaustive]` decision as an open question.
- **Rationale:** Correctness — `u64` is a first-class schema type and must round-trip without loss.
  Since a `types` major is on the table anyway, this is the moment to also sweep in any other pending
  `types` breaks so external users take **one** upgrade.
- **Scope if actioned:** 1 file (`crates/types/src/lib.rs`) — 2 variants, `type_name` arms,
  `is_numeric`, 2 `From` impls; update `crates/types/README.md` + examples/tests that enumerate
  variants. No codegen change required (audit `crates/crud-api` for any `Value` construction as a
  cheap safety check).

### S4 — per-append fsync
- **Current state:** **Confirmed.** Every mutating method calls `self.file.sync_all()`:
  `append_u32/u64/i32/i64/f64/bool/uuid/timestamp/bytes` (`crates/storage/src/lib.rs:229, 253, 277,
  301, 325, 349, 373, 397, 429`), `VariableColumn::append_string` fsyncs **both** data and offsets
  files (`:510, :517`), and `Tombstones::append` (`:591`). A single N-column row insert therefore
  issues **N + 1** fsyncs (plus a second for each string column) — this dominates insert latency and
  defeats OS write coalescing. The generated `insert()` appends column-by-column
  (`crates/codegen/src/rust.rs:332-347`), so the cost scales with column count on every row.
- **Break characterization:** None. Removing the internal `sync_all()` calls and adding an explicit
  `flush(&mut self) -> io::Result<()>` (fsync-all-open-handles) is purely additive to the public API;
  no signature changes. It **is** a durability *behavior* change: after the change, a crash between
  `flush()` calls can lose the most recent appends to the columnar files.
- **Options:**
  - *Batched durability with WAL as the boundary (chosen)* — drop per-append fsync; the columnar
    files become a rebuildable materialization and the **WAL** (`open_with_wal` + `FsyncPolicy`,
    already present at `:663-697`) is the crash-durability contract. Add `flush()` for explicit
    checkpoint/close and call it at commit boundaries.
  - *Configurable `FsyncPolicy` on `FixedColumn`/`VariableColumn` (`Always` | `Batch` | `Never`)* —
    mirrors WAL's policy for parity; more surface area. Fold in later if a per-column knob is wanted.
  - *Keep per-append fsync* — simplest durability story but the performance floor is unacceptable for
    multi-column inserts. Rejected.
- **Decision:** **Drop per-append fsync; add `flush()`; make WAL the durability boundary.** Ship in
  the same `storage` 0.1.2 as S9.
- **Rationale:** The append path exists precisely so the WAL can be the source of truth; syncing every
  column write duplicates that guarantee at a large cost. This is the single biggest write-path win
  available and it is non-breaking.
- **Scope if actioned:** 1 file (`crates/storage/src/lib.rs`) — remove ~12 `sync_all()` calls, add
  `FixedColumn::flush`, `VariableColumn::flush`, `Tombstones::flush`; generated `insert()`/close path
  should call `flush()` at the commit boundary (`crates/codegen/src/rust.rs`, re-accept snapshots).
  Add/adjust a durability test asserting data is readable after `flush()`. Document the durability
  contract change in `crates/storage/README.md`.

### S5 — type-aware file naming
- **Current state:** The `Database` path **helpers** are type-blind — `fixed_column_path` always emits
  `fixed/u64_{index}.bin` regardless of the real column type (`crates/storage/src/lib.rs:748-761`).
  However, **generated code does not use these helpers**: `RustGenerator` builds its own paths with
  the real type name, `"{model}/fixed/{type_name}_{index}.bin"` (`crates/codegen/src/rust.rs:184-204`
  via `Self::type_name`). The only callers of the `Database` helpers are
  `crates/storage/examples/basic_usage.rs` and the README. So the on-disk format that ships in
  generated databases is *already* type-aware; the wart is confined to a misleading, largely-unused
  helper.
- **Break characterization:** No on-disk break for real (generated) databases — their naming is
  unchanged. Fixing the helpers to be type-aware requires a `ColumnType` argument, which would change
  their signatures (breaking) — **but** we can instead add new type-aware helper methods and
  `#[deprecated]` the old ones, keeping the change additive. Blast radius is examples + README only.
- **Options:**
  - *Add `fixed_column_path_typed(index, ColumnType)` (+ variable variants), deprecate the old
    `u64_`-hardcoded helpers (chosen)* — non-breaking, aligns helpers with what generated code already
    does, improves debuggability (file names disclose column type).
  - *Change existing helper signatures in place* — cleaner names but breaking for the example; would
    push `storage` to 0.2.0 for negligible benefit. Rejected.
  - *Leave as-is* — acceptable, since generated code is unaffected; the cost is a permanently
    misleading helper. Do it only if we touch storage anyway (we are, for S9/S4).
- **Decision:** **Do it additively, low priority** — new typed helpers + deprecate old, folded into
  `storage` 0.1.2. Verify generated naming is unchanged (it is) to guarantee no migration.
- **Rationale:** Safety/debuggability win at near-zero risk; avoids a signature break by going
  additive. Lowest value of the four, so it rides along rather than driving a release.
- **Scope if actioned:** 1 file (`crates/storage/src/lib.rs`) — 3 new typed helpers + `#[deprecated]`
  on 3 old ones; update `crates/storage/examples/basic_usage.rs` and `README.md`. No generated-code
  or on-disk-format change.

## Versioning & sequencing plan
Independent version lines are preserved — **no normalization**.

- **`types` → 0.2.0 (major).** Driven solely by T3 (new `Value` variants). Because this is the one
  forced major, treat it as the batching point for **any** other pending `types` break so external
  users absorb a single upgrade. Ship first, because codegen and storage both depend on `types`.
- **`storage` → 0.1.2 (minor).** Batches S9 (compatible relaxation), S4 (additive `flush()` +
  durability behavior change), and S5 (additive typed helpers). None of these is a hard semver break,
  so a major bump is **not** warranted. The durability behavior change (S4) is the one thing to flag
  loudly in the changelog even though it is API-compatible.
- **`wal` → unchanged (stays 0.1.1).** Nothing in this proposal touches `wal`'s API or format.

**Order of operations:**
1. Cut `types` 0.2.0 (T3). Bump the workspace dependency on `types` to `0.2`.
2. Land `storage` S9 + S4 + S5 together; publish `storage` 0.1.2.
3. **Retarget codegen** against the shipped APIs: generated `get()` → `&self` (S9); call `flush()` at
   the insert/commit boundary (S4). Re-accept all `crates/codegen/tests/snapshots/*.snap`, then
   **compile the emitted Rust in a throwaway crate** (snapshot pass ≠ output compiles — per the
   codegen caveat). T3 needs no codegen change (generated structs use native `u32`/`u64`; codegen
   never constructs `Value`), but audit `crates/crud-api` for stray `Value` construction.

## Proposed impl tasks
1. `types`: add `Value::U32(u32)` + `Value::U64(u64)`; extend `type_name`, `is_numeric`; add
   `From<u32>`/`From<u64>`; update README + examples/tests. (Decide `#[non_exhaustive]` first — see
   open questions.)
2. `types`: bump to 0.2.0; update workspace dependents to `types = "0.2"`; sweep in any other pending
   `types` breaks so the major is spent once.
3. `storage` S9: convert `FixedColumn`/`VariableColumn`/`Tombstones` readers to positional I/O
   (`read_at`/`seek_read`) with `&self` receivers.
4. `storage` S4: remove all per-append `sync_all()` calls; add `flush()` to each storage struct;
   document WAL-as-durability-boundary in README; add a post-`flush()` readback test.
5. `storage` S5: add type-aware path helpers, `#[deprecated]` the `u64_`-hardcoded ones; update the
   example + README; assert generated on-disk naming is unchanged.
6. `storage`: bump to 0.1.2; changelog must call out the S4 durability behavior change.
7. `codegen`: flip generated `get()` to `&self`; emit `flush()` at the commit boundary; re-accept
   snapshots; compile-check emitted Rust in a throwaway crate; audit `crud-api` for `Value` use.

## Open questions for the user
- **Cut `types` 0.2.0 now, or hold for a batch?** T3 is currently the *only* known `types` break. Is
  it worth spending a major on T3 alone now, or should T3 wait until at least one more `types` break
  is queued so the major carries more? (Recommendation: proceed now — `u64` losslessness is a
  correctness gap, and real external users of a 0.x crate are likely few.)
- **Mark `Value` `#[non_exhaustive]` while we are breaking it?** It would spare future variant
  additions from being breaking, at the cost of forcing wildcard arms on downstream matches now.
  Worth it only if we anticipate more `Value` variants soon.
- **S4 durability posture:** is "columnar files are rebuildable, WAL is the durability contract"
  acceptable as the default, or do you want a configurable `FsyncPolicy` on the column types from day
  one (more surface, WAL-parity) rather than deferring it?
- **Confirm the low external-user assumption.** The whole "S9/S5 additive, T3 cheap" calculus rests on
  there being essentially no external consumers pattern-matching these APIs. If `types`/`storage` have
  known downstream users outside this workspace, the T3 break and the S9 relaxation both deserve a
  louder migration note.
