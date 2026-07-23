# Register: deeper (Tier 2 — "Dive deeper")

Load with `spine.md`. This governs the collapsed **"Dive deeper"** block a reader opens
when the terse body made them curious about one thing.

**Target: 5–6 on the technical scale.** Engineer-to-engineer, but explaining rather than
specifying. The register of a good design note or a thoughtful PR "why" section.

## What deeper is for

The reader accepted the terse claim and now wants to *understand* it — the mechanism at a
conceptual level, and how this piece fits the rest of the system. This is where the
corpus's tradeoff-first instinct shines: name what the design buys, what it costs, and why
the cost is acceptable.

- Explain the **how** at the level of concepts and ordering, not byte layouts.
  ("The WAL is fsynced before columns are touched, so recovery always has an authoritative
  copy to replay" — yes. The CRC frame format — no, that's technical.)
- Draw the **connections**: how this feature relates to snapshots, the change feed, the
  single-writer contract, etc.
- Give the reasoning behind a limit the terse body stated. The *limit* was terse; its
  *rationale* lives here.

## Relationship to the terse body

- **Add, never repeat.** Assume the reader just read the terse body. Don't re-explain what
  the thing does — explain why and how.
- Do not re-state the limit as if it were news; deepen it (why it exists, what it would
  take to lift it).

## What deeper is *not* for

- Not exact invariants, file formats, syscall-level specifics, or "the code does X on line
  Y." That's Implementation details (7–10).
- Not a second full telling of the feature. It's a focused expansion on the point the
  terse body raised.

## How it reads

- Full sentences and short paragraphs; a diagram or a small table is welcome.
- One concrete number or identifier where it sharpens the point, not a spec sheet of them.
- Still subject to the spine: em-dash discipline, emphasis budget, no worn refrains.

## Example (terse claim → deeper expansion)

**Terse body said:** "Writes are crash-safe; killing the process mid-write never loses a
saved row."

**Dive deeper (5–6):**
> The guarantee comes from ordering. Each model has a write-ahead log, and a write appends
> its record to that log and flushes it to disk *before* any column file is touched. So if
> the process dies in between, recovery finds a logged record with a missing or half-written
> column tail, trims the partial tail back to the last consistent row, and replays the
> logged record. The cost of this safety is a disk flush on the write path — the single
> biggest write-latency lever, which is why it's configurable (see `[storage].fsync`).
