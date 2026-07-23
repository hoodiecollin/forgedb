# In-browser prose rewrite (LOCAL DEV ONLY)

Highlight prose on a rendered `/docs/**` page and have Claude Code rewrite the
backing `.mdx` live. Everything here is gated to `NODE_ENV === "development"` and
never ships in the static export.

## Run it

```bash
make website              # dev server on http://localhost:3100
make website-rewrite-watch   # (in Claude's session) wake watcher — see below
```

Open any docs page, press **⌥E** (or click the **Rewrite** button, bottom-right).

- **Click a paragraph / list / callout** → rewrite that block.
- **Click a heading** → rewrite the whole section (toggle to just the heading).
- **Drag-select text** → rewrite that span (toggle to the enclosing block).

Type an instruction (or a preset chip), pick **Diff** (one rewrite) or **3
options**, and **Send**. Claude drafts a proposal; review the diff / candidates
and **Accept** to splice it into the `.mdx`. The page reloads to show the change.

## How the loop works

```
overlay ──POST /api/dev-rewrite──▶ .rewrite-queue/requests.jsonl + briefs/<id>.md
   ▲                                        │ (rewrite-watch.ts exits ─▶ wakes Claude)
   │ short-poll GET                         ▼
   └───── proposals/<id>.json ◀── Claude reads brief, writes proposal
   Accept ──▶ route splices candidate into content/**.mdx ──▶ reload
```

- **Source mapping** — `remark-source-map.ts` stamps `data-src-start/end`
  (content-space char offsets) on every block; `rewrite-target.ts` turns a
  click/selection into a source range.
- **Staleness guard** — the page stamps a hash of the MDX body; if the file
  changes after render, the offsets are stale, so the route refuses the
  submit/accept (409) rather than corrupt the file. Reload to continue.
- **Style loader** — style lives in `content/style/`: a shared `spine.md` plus one
  register per tier (`terse.md` 2–3, `deeper.md` 5–6, `technical.md` 7–10). Each
  request's brief composes `spine + <tier register>` (`rewrite-style.ts`), keyed off
  the target's tier and the page's `purpose`/`structure` frontmatter. Edit the style
  files to steer output; changes take effect on the next request.
- **Brief** — when a request lands, the route writes `.rewrite-queue/briefs/<id>.md`
  (`rewrite-brief.ts`): the request context, the exact source slice, the composed
  style, grounding rules, and the required proposal shape — everything the generator
  needs in one file. Regenerate on demand with `bun scripts/rewrite-brief.ts <id>`.
- **Page frontmatter** — `purpose: orientation|reference|marketing` and (Build-C only)
  `structure: "C"` drive register strictness and the two-body warning. See
  `content/style/spine.md`.

## Files

| File | Role |
|------|------|
| `remark-source-map.ts` | stamps source offsets onto blocks (dev-only remark plugin) |
| `rewrite-target.ts` | DOM click/selection → source target (section/block/span) |
| `rewrite-types.ts` | shared protocol types |
| `rewrite-queue.ts` | fs-backed queue + splice + staleness re-check |
| `rewrite-style.ts` | style loader — composes `spine + <tier register>` |
| `rewrite-brief.ts` | per-request generation brief → `.rewrite-queue/briefs/<id>.md` |
| `rewrite-hash.ts` | content fingerprint for the staleness guard |
| `rewrite-atoms.ts` | jotai state (mode, feedback default) |
| `../../components/dev/rewrite-overlay.tsx` | the overlay UI |
| `../../app/api/dev-rewrite/route.ts` | **gitignored** POST/GET route (breaks `output: export`) |
| `../../scripts/rewrite-watch.ts` | wake watcher |

The route handler is gitignored because a POST handler is incompatible with the
site's `output: "export"` production build. `next.config.ts` scopes the export to
production so the route works under `next dev`.

`make website-build` runs through `scripts/website-build.ts`, which stashes the
route out of the `app/` tree for the build and restores it after (even on failure
or ctrl-C), so a **local** export build just works. On CI the route is absent, so
it's a plain `next build`.
