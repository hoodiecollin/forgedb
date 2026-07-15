# ForgeDB Inspector — design review & corrections

Review of the design agent's output (claude.ai/design project
`ba160e30-38a3-40a5-9e3b-97abfbf9d27b`), captured for our own use. **We are not
returning to the design agent** — these are the corrections we apply ourselves when the
mockup graduates into the real Tauri implementation (issue #63).

## Artifacts reviewed

- `ForgeDB Inspector - Directions.dc.html` — three greyscale wireframe directions
  (**1a Studio** grid-first, **1b Console** query-first, **1c Atlas** relation-graph +
  structure/live lens).
- `ForgeDB Inspector.dc.html` — the built prototype. It **synthesized all three** into one
  shell: top-nav Atlas / Studio / Console / Dashboards + a shared record-editor drawer,
  backed by real `DCLogic` state, rendering the real InspectorKit components, dark-forced,
  status palette throughout.

## What is domain-correct (keep as-is)

- **Type→control mapping**, including the hard cases: nullable-scalar explicit NULL toggle
  ("value absent, distinct from empty"); tri-state bool only when `?`; `u64`→string input
  with JS-safe-integer warning; `timestamp`→dual unix-ms/human, disabled when `+`;
  `char(N)`→maxlength + byte counter; `uuid +`→read-only system-managed; `*FK`/`?FK`→Select
  picker with correct required/optional notes; `struct`→nested subfields + set-whole-to-null.
- **Honest limits surfaced as UI copy** (all real, from CLAUDE.md's deferred list): M2M "no
  unlink operation exists"; has-many "reverse lookup is a linear scan"; "Update replaces the
  whole record" / **Save (replace)** (superseding-version append, not partial update).
- **Verb surface** semantically right: snapshot = "consistent point-in-time across all
  models" (`DatabaseSnapshot`); live deltas Added/Updated/Removed (`LiveDelta`); the
  **Structure (at-rest, reads files) vs Live (attached, needs API)** lens as a top-level
  control.
- **Identity: green.** Nothing implies a runtime schema-reflection engine — all surface reads
  as generated-per-schema; the inspector attaches to the running generated API. Structure
  lens reads columns/manifest at rest (backup substrate), schema-agnostic.

## Corrections to apply during implementation

1. **No `text` type.** The mock's "text / multiline" control is a rendering heuristic over
   `string` (keyed off `@length`/`@fulltext`), and it correctly keeps `typeLabel: "string"`.
   Keep it a heuristic — the spec must not introduce a `text` schema type. Multiline is a UI
   choice, not a type.
2. **`@fulltext` is semantic-only.** The directive badge is fine, but it must not imply a
   working full-text backend — the `fulltext` crate was removed in Phase 3b. No search UI may
   claim to execute against it until/unless real fulltext lands.
3. **Snapshot "compare vs current" diff** and the **Dashboards** screen are *inspector-level
   constructs*, not engine features. Legitimate tooling (they compose generated reads/queries
   client-side), but label them "tool builds this," not "ForgeDB provides this." The diff is a
   client-side comparison of two point-in-time reads, not an engine capability.
4. **Filter predicate ops** (`.eq`/`.gte`/`~contains`) are illustrative. The real surface is
   the **closed compile-time per-field predicate set** the generator emits — bind the
   composer to that generated set, never a free-form predicate parser. The index-vs-scan chip
   distinction is good and should be driven by which fields carry `^`.
5. **`@default` is semantic-only today.** The mock pre-fills defaults as a UI convention;
   that's fine, but don't wire it to any runtime default-application behavior that doesn't
   exist.

## Open product decision (ours, before implementation)

The agent **merged all three directions into one four-screen shell** — more surface than a
first build needs. Decide whether v1 leads with **Studio** (grid-first: fastest, densest,
covers read/filter/edit) and folds Console / Atlas / Dashboards in later, or commits to the
unified shell now. Recorded on #63.

## Known DS gap

The **relation graph is a hand-composed mock** — InspectorKit ships no graph/DAG component.
The agent self-flagged this as a **SPIKE**. Tracked as its own issue (graph-library research)
and linked from #63.
