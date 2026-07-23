# Style spine — shared rules

The shared voice for all ForgeDB docs prose. Every rewrite loads **this file plus
exactly one register file** (`terse.md`, `deeper.md`, or `technical.md`) for the tier
of the block being rewritten. This file is the constitution; the register file sets the
depth. When they seem to conflict, the register file wins on *depth*, this file wins on
*voice and integrity*.

## Who is reading

One audience — engineers fluent in databases and programming — wearing different hats,
often the same person minutes apart:

- **Evaluating** — skimming to decide if ForgeDB is worth digging into.
- **Casually reading** — more than skimming, less than studying.
- **Deep-reading** — building a real mental model to use or trust it.
- **Referencing** — using a page as a manual, looking up one exact thing.

Assume database and programming literacy. Do **not** assume prior ForgeDB knowledge, and
do not assume the reader has read any other page. Terse tiers serve the first two hats;
deeper and technical tiers serve the last two.

## The technical scale (1–10)

Referenced by every register file. 1 = plain business English, no jargon. 10 = a systems
manual assuming Rust and storage-internals fluency.

- **2–3** — Tier 1 page body (`terse.md`). Plain English; unavoidable technical nouns only.
- **5–6** — "Dive deeper" blocks (`deeper.md`). The *why* and *how it fits the system*.
- **7–10** — "Implementation details" blocks and Build-C detailed bodies (`technical.md`).
  Internals, invariants, exact mechanisms.

## Voice to preserve

The corpus already does these well. Keep them.

- **Volunteered honesty.** State limits, gaps, and losses plainly and early. When a
  competitor or alternative wins, say so and say why. Trust is the product's best asset.
- **Tradeoff-first.** Frame a design choice as *what it buys and what it costs*, together.
  Never present an upside without its price.
- **Quantified claims.** Prefer a number to an adjective. "~2.6× smaller than SQLite,"
  not "much smaller." Real HTTP codes (422/409/503), real latencies, real byte sizes.
- **Confident and direct.** Present tense, active voice, second person. "A write appends,"
  "you supply the id." State facts; don't hedge (the *only* hedging allowed is a genuine
  limit disclosure).
- **Opinionated where it helps.** Name anti-patterns and give the better path
  ("prefer moderate batches over one giant transaction").

## Hard rules — integrity

**1. Honesty stays in Tier 1.** A *stated limitation* is always terse-tier content:
read-only, single-writer, "not yet exercised," "returns 409," "not auto-incremented." You
may move the *mechanism or rationale* of a tradeoff into a deeper/technical tier; you may
**never** move the *existence of the limit* out of Tier 1. Terse must never read as
marketing because the caveats got demoted. If a rewrite would remove a limit from a terse
block, stop — that's a violation.

**2. Don't restate what the code says.** Prose earns its place by explaining *why* and
*how it fits*, not by narrating a code sample line by line. A `curl` block with a `# →`
output comment needs a sentence of context, not a paraphrase of the flags.

**3. No marketing vocabulary.** Never: *powerful, blazing(-fast), lightning-fast,
seamless, effortless, robust, world-class, cutting-edge, revolutionary, game-changing,
supercharge, unlock, leverage* (as a verb), *magic, delightful, simply, just* (as
hand-waves that hide difficulty). If something is good, prove it with a number.

## Hard rules — mechanics

**4. Em-dash discipline.** At most **one** em-dash per sentence, and not in every
sentence. The corpus overuses them; a comma, a period, or a parenthesis is usually
better. Never stack two em-dash asides in one sentence.

**5. Emphasis budget.** At most **one** bolded span per paragraph, and only for a genuine
pivot the reader would otherwise miss (a *not*, a *before/after* ordering, a hard limit).
When everything is bold, nothing is. Prefer sentence structure over bold to carry
emphasis.

**6. Refrains — scale avoidance by page `purpose`, don't just reword them.** A refrain (a
signature phrase repeated to drive an idea home) is a *teaching* device: it only earns its
place while the reader hasn't internalized the idea yet. That familiarity tracks page
purpose, so the strictness does too. Read the page's `purpose` frontmatter and apply:
  - **marketing** — refrains have value. Repetition reinforces the pitch for a reader
    skimming fast. Keep them.
  - **orientation** — reduced value, not zero. The reader may be new and may not have seen
    the phrase elsewhere (they might have skipped the marketing page). State the idea once,
    where it lands, in the words the page needs — then move on. Don't repeat the same
    refrain within a page and don't lean on it as a crutch.
  - **reference** — avoid. By now the reader knows the concept cold; a refrain is noise in
    front of the lookup they came for. Prefer a link to the canonical statement over
    restating it.

  Rewording is **not** the fix — a fresh synonym for the same tic is still a tic. When the
  idea genuinely needs stating, say the plain thing (*"load-bearing"* → name what actually
  depends on it: "recovery relies on this order"). The worn phrases to watch: *"load-bearing,"
  "the whole story,"* *"honest/honestly"* as a self-congratulatory tag (just state the
  caveat — don't announce that you're being honest), and the identity mantra (*"a
  compile-time input to generation, never a runtime input to a generic engine"*). The mantra
  is the strictest case even in orientation: state the invariant once, in the page's own
  words — never paste the stock sentence verbatim.

**7. One idea per sentence.** Break run-ons. If a sentence carries three or more
qualifications stacked with commas, dashes, and parentheses, split it. Density is a virtue
only until it needs re-reading.

**8. Don't drop the reader off a cliff.** The first time a page uses a ForgeDB-specific
term of art (*watermark snapshot, torn-tail recovery, superseding-version append,
substrate*), give a half-clause of grounding or a link. Fluency in databases ≠ fluency in
ForgeDB's internal vocabulary.

## Mechanical conventions

- **Links:** internal links carry a trailing slash (`/docs/schema/relations/`) and read as
  noun references, not "click here."
- **Code identifiers** are inline code: `truncate_to_rows`, `FsyncPolicy::Always`, `+uuid`.
- **Callouts** carry a full-assertion title, not a label. `warning` = danger or a hard
  limit; `note` = a subtlety or clarification; `tip` = practical guidance. A limit
  disclosure in a callout still counts as Tier-1 content (rule 1).
- **Numbers** keep their units and their comparison baseline (`37 µs`, `~2.6× smaller than
  SQLite`). A bare number with no baseline is not a claim.
