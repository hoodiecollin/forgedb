# Register: technical (Tier 3 — "Implementation details" + Build-C detailed bodies)

Load with `spine.md`. This governs two things with the same voice:

1. The optional collapsed **"Implementation details"** block on a Build-B page.
2. The **detailed-native body** of a Build-C page (rendered when the reader's global
   preference is "detailed").

**Target: 7–10 on the technical scale.** A systems manual. Assume Rust fluency and
storage-internals fluency. Exact, complete, unafraid of `xmin`/`xmax`-level specifics.

## The earn-its-place gate (Tier 3 only)

An "Implementation details" block is **optional and rare**. It exists **only** when there
is genuinely novel or interesting internal content *beyond* what Dive deeper already
covered. If the block would just restate the deeper explanation with more words, or narrate
the obvious, **it should not exist** — an absent Tier 3 is the default. Do not manufacture
depth to fill the slot; that is exactly the filler the corpus avoids.

Good reasons for a Tier-3 block: an invariant that isn't obvious from the API, a
correctness argument (why an ordering is safe under crash), a non-obvious edge case, the
exact on-disk format, a subtle interaction between two subsystems.

(This gate does not apply to Build-C detailed bodies, which are always present — they are
the page for a reader who asked for the full technical telling.)

## What technical is for

- **Invariants and why they hold.** "Every committed column's byte length is a pure
  function of row count plus fixed layout, so recovery derives the durable prefix from file
  lengths and no persisted checkpoint marker is required for correctness."
- **Exact mechanisms.** Frame formats, offsets, syscalls, the specific order of operations
  and why reordering breaks.
- **Correctness arguments.** Walk the crash windows; show what survives each and why.
- **Named internals.** `truncate_to_rows`, `_txn_journal.log`, `FsyncPolicy`,
  `PersistedEvent { offset, model, row_index, kind, bytes }` — the real symbols.

## Still bound by the spine

Depth is not license to drift.

- **Rule 1 still holds:** a limit's *existence* was already stated in Tier 1. Here you give
  its full mechanism and edge cases — you do not "reveal" a limit that terse hid.
- **Refrains still governed by spine rule 6 (scaled by page `purpose`):** depth is not a
  license to repeat. On a reference page, avoid the worn phrases; on an orientation page,
  state the idea once. A manual earns trust from exact nouns, not from "load-bearing" five
  times.
- **Em-dash and emphasis budgets still apply.** Precision comes from exact nouns and
  correct ordering, not from dashes and bold.
- **Quantify.** This is where the real measurements live (3.89 ms barrier, 66% reclaim).

## How it reads

- Longer paragraphs are fine; the reader opted in. Still one idea per sentence.
- Diagrams, tables, and annotated code carry weight here — an ASCII layout of the on-disk
  directory, a state table of the recovery cases.
- It should read like the best of `docs/ARCHITECTURE.md`: complete, exact, and trusting the
  reader to keep up — without the worn refrains or the em-dash pile-ups.

## Example (Implementation details block)

**Deeper said:** recovery "trims the partial tail back to the last consistent row and
replays the logged record."

**Implementation details (7–10):**
> Recovery runs per model in `recover_from_wal` on `Database::open_at`. It computes the
> minimum-consistent row count `n` such that every column file's length equals
> `n × fixed_width` (fixed columns) or matches the `n`-th offset entry (variable columns),
> then calls `truncate_to_rows(n)` to discard any torn tail. It replays the WAL by absolute
> row index starting at `n`, which is idempotent: a record whose columns already reached
> disk re-appends to the same index and is a no-op. Because `n` is derived from file lengths
> rather than a persisted LSN, a checkpoint that fsyncs columns then truncates the WAL
> leaves recovery correct with no durable marker — the checkpoint order (columns durable,
> *then* WAL truncated) is the only part that cannot be reordered.
