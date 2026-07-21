---
name: rfc-workflow
description: Capture a ForgeDB design, proposal, or plan as an `rfc`-labeled GitHub issue instead of a committed file. Use whenever you're about to write a design doc / proposal / plan under docs/ (or anywhere in the tree), when you catch yourself creating a DESIGN/PLAN/RFC/proposal markdown file, or when a discussion produces a decision worth recording. Keywords: proposal, design doc, RFC, plan, spec, request for comment, design note.
---

# rfc-workflow

ForgeDB does **not** commit design docs. Proposals, design notes, and plans drift
the moment the code moves past them, and a stale doc in the tree reads as
authoritative long after it's wrong. So they don't live in the repo — they live
as **`rfc`-labeled GitHub issues**, where they can be discussed, superseded, and
closed without leaving a fossil behind. This skill is the discipline that keeps
that true, plus the exact procedure for filing one.

This is the documentation counterpart to the `forgedb-product-manager` gate: the
PM agent guards *what* gets built (generator identity); this skill guards *where
the thinking about it lives* (an issue, never a committed file).

## The one rule

**A forward-looking design/proposal/plan is an `rfc` issue, not a file.** The only
docs that belong in the tree are **descriptions of shipped behavior** — user
guides (`docs/SCHEMA.md`, `docs/MIGRATIONS.md`, …) and the durable architecture
reference (`docs/ARCHITECTURE.md`). If a doc describes something that *doesn't
exist yet* or argues for *a direction*, it's an RFC and it goes to GitHub.

Ground truth for the backlog is `gh issue list`, not a file. (This mirrors the
project's other disciplines — see CLAUDE.md → *Operating disciplines* #5.)

## The gate — catch it before it lands in the tree

You are about to violate the rule if you find yourself:

- creating `docs/proposals/…`, `DESIGN*.md`, `PLAN*.md`, `SPEC*.md`, `RFC*.md`, or
  an `*-impl-plan.md` / `*-design-brief.md` / `*-design-review.md`;
- writing a markdown file whose content is "here's what we *should* build" or
  "here are the options for X" rather than "here's how X *works*";
- pasting a long design discussion into any committed file to "save it."

When you catch this: **stop, and file an RFC issue instead** (procedure below).
If the thing genuinely describes shipped behavior, it's not an RFC — put the
durable part in `docs/ARCHITECTURE.md` and skip the rest.

## Before filing — don't duplicate

Almost every forward-looking item is *already* tracked. Check first:

```bash
gh issue list --search "<keywords>" --state all --limit 20
gh issue list --label epic --state open        # is there a parent epic?
```

- If an **open issue already covers it**, add your design as a comment there
  instead of opening a new one.
- If it's **Phase N of an existing epic**, file the RFC and link it *under* that
  epic (comment on the epic; reference the epic in the RFC body). Example: RFC
  #172 (storage-model Phase 2) sits under epic #167.
- Only open a fresh RFC when nothing tracks the idea yet.

## Filing the RFC issue

Use the `rfc` label (create it once if a repo doesn't have it):

```bash
# one-time, only if `gh label list --search rfc` shows nothing:
gh label create rfc --color 5319e7 \
  --description "Request for comment: design captured as an issue (proposals no longer committed to the repo)"
```

Add the domain labels that apply (`perf`, `experiment`, `config`, `idea`,
`tech-debt`, …) alongside `rfc` — an RFC is still categorized by what it touches.

Body template (keep it tight; an RFC is a decision aid, not a spec dump):

```markdown
## Context
What exists today and why this is worth discussing. Link the epic / prior issues.

## Problem / motivation
The concrete gap or question. What breaks or is missing without this.

## Proposed design
The direction being proposed, at the level of detail needed to evaluate it.
Name the invariant it must not break (generator identity: schema stays a
compile-time input; published artifacts stay schema-agnostic substrate or
transport glue).

## Alternatives considered
The other options and why they're worse — this is the "for comment" part.

## Open questions
What's genuinely undecided and needs input before committing.
```

File it and cross-link the parent epic:

```bash
gh issue create --label rfc --label <domain> \
  --title "RFC: <concise title> (Phase N of #<epic>)" \
  --body-file <path>            # write the body to the scratchpad, not the repo

gh issue comment <epic> --body "Filed RFC #<n> for <thing>: <url>"
```

Write the body to the **scratchpad**, never into the repo tree — the whole point
is that it doesn't get committed.

## Lifecycle — what happens after "for comment"

An RFC resolves one of two ways, and both end with the issue *closed* so the
backlog stays honest:

- **Accepted → built.** Implement it. Then distill the *durable* architecture
  (how the shipped thing works, the invariant it preserves) into
  `docs/ARCHITECTURE.md`, and **close the RFC** referencing the commit(s). The
  design discussion stays in the closed issue; the tree gains a description of
  *what now exists*, not a plan for what might. (This is exactly how the
  multi-writer-MVCC proposal became the "control plane vs data plane" paragraph
  in ARCHITECTURE.md.)
- **Rejected / superseded.** Close with a comment saying why, linking whatever
  replaced it. Nothing lands in the tree.

Never leave an accepted RFC's design as a committed file "for reference" — the
ARCHITECTURE.md distillation *is* the reference; the issue holds the rationale.

## Close with a note

Tell the user what you did: the issue number + URL, which epic it's linked under
(or which existing issue you commented on instead of opening a new one), and — if
you redirected something away from a committed file — say so, so the discipline
is visible rather than silent.
