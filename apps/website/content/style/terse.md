# Register: terse (Tier 1 — the page body)

Load with `spine.md`. This governs the **default page body** a reader sees before
expanding anything, and the terse-native body of a Build-C page.

**Target: 2–3 on the technical scale.** Plain English. The register a smart non-specialist
colleague would understand — a product manager who codes, an engineer in evaluate-mode who
hasn't decided to care yet.

## What terse is for

The reader is deciding whether ForgeDB solves *their* problem and whether to keep reading.
Answer that. Lead with the outcome and the "why it matters," not the mechanism.

- Say what the thing **does for you** and **when you'd reach for it**.
- Name the shape of the tradeoff in one plain sentence; leave the mechanics to Dive deeper.
- State every hard limit that affects a decision (rule 1 — limits are never demoted).

## What terse is *not* for

- No internals. No file formats, byte layouts, fsync-ordering arguments, or invariants.
  That is Dive deeper (5–6) or Implementation details (7–10).
- No walking through a code sample. One short example is fine; don't narrate it.
- No stacked qualifications. If you need three clauses of nuance, the nuance belongs deeper.

## How it reads

- Short sentences. One idea each.
- Prefer a concrete everyday framing over a precise-but-abstract one. "Two people can't
  write to the same data at once" beats "the directory lock enforces single-writer
  mutual exclusion" — save the precise version for deeper.
- Keep the one number that a skimmer would actually weigh (the 100× write span, the
  footprint ratio). Drop the rest.
- Technical nouns only when there's no plain substitute, and ground them the first time.

## Examples (dense corpus → terse)

**Durability, before (≈6):**
> Every `insert`/`update`/`delete` durably records what it is about to do **before** it
> touches the columnar storage files, so a crash — even `kill -9` mid-write — never loses
> an acknowledged row and never corrupts the data directory.

**Terse (2–3):**
> Writes are crash-safe. If the process is killed mid-write, you never lose a saved row and
> the data never corrupts. How that's guaranteed is in Dive deeper.

**Browser replica, terse:**
> Run the same database in the browser as a read-only local copy that stays in sync with
> the server. It's for local querying and offline reads — not for going faster than a
> normal API call. If you just want speed, a cached API client is the lighter choice.

Note the limit ("read-only," "not for going faster") stays in the terse body. That is the
rule, not an option.
