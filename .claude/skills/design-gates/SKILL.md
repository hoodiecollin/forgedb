---
name: design-gates
description: "Capture a ForgeDB design, proposal, or plan as the design GATE sub-issue of a work item instead of a committed file. Use whenever you're about to write a design doc / proposal / plan under docs/ (or anywhere in the tree), when you catch yourself creating a DESIGN/PLAN/RFC/proposal markdown file, or when a discussion produces a decision worth recording. Keywords: proposal, design doc, RFC, plan, spec, request for comment, design note, gate."
---

# design-gates

ForgeDB does **not** commit design docs. Proposals, design notes, and plans drift
the moment the code moves past them, and a stale doc in the tree reads as
authoritative long after it's wrong. So they don't live in the repo — they live
as **gate sub-issues**, where they can be discussed, superseded, and closed
without leaving a fossil behind. This skill is the discipline that keeps that
true, plus the exact procedure.

This is the documentation counterpart to the `forgedb-product-manager` gate: the
PM agent guards *what* gets built (generator identity); this skill guards *where
the thinking about it lives* (a gate issue, never a committed file).

> **This replaced the `rfc` label.** There is no `rfc` label any more — a design
> is gate 1 of the work item it designs, so it is always attached to the thing it
> is about rather than floating beside it. If you are looking for an old RFC, it
> is a closed issue in the history.

## The one rule

**A forward-looking design/proposal/plan is a gate issue, not a file.** The only
docs that belong in the tree are **descriptions of shipped behavior** — user
guides (`docs/SCHEMA.md`, `docs/MIGRATIONS.md`, …) and the durable architecture
reference (`docs/ARCHITECTURE.md`). If a doc describes something that *doesn't
exist yet* or argues for *a direction*, it belongs in a gate.

Ground truth for the backlog is `gh issue list`, not a file. (This mirrors the
project's other disciplines — see CLAUDE.md → *Operating disciplines* #5.)

## The gate — catch it before it lands in the tree

You are about to violate the rule if you find yourself:

- creating `docs/proposals/…`, `DESIGN*.md`, `PLAN*.md`, `SPEC*.md`, `RFC*.md`, or
  an `*-impl-plan.md` / `*-design-brief.md` / `*-design-review.md`;
- writing a markdown file whose content is "here's what we *should* build" or
  "here are the options for X" rather than "here's how X *works*";
- pasting a long design discussion into any committed file to "save it."

When you catch this: **stop, and write it into the design gate instead**
(procedure below). If the thing genuinely describes shipped behavior, it is not a
design — put the durable part in `docs/ARCHITECTURE.md` and skip the rest.

## Step 1 — find the work item (don't duplicate)

A gate belongs to a work item, so find or file that first. Almost every
forward-looking item is *already* tracked:

```bash
gh issue list --search "<keywords>" --state all --limit 20
gh issue list --label epic --state open        # is there a parent epic?
```

- If an **open work item already covers it**, use its gate — do not open a second
  issue.
- If it's **Phase N of an existing epic**, file the work item as a sub-issue of
  that epic.
- Only file a fresh work item when nothing tracks the idea yet:

```bash
gh issue create --label improvement --title "<concise title>"
```

Every work item carries exactly one type — `improvement`, `bugfix`, or
`experiment` — and the type decides its gates.

## Step 2 — materialize the gates

**Never create a gate issue by hand.** `materialize` owns them and creates a
complete set at once; a hand-made gate destroys the only thing that makes an
*absent* gate meaningful. The `PreToolUse` hook will refuse a manual
`gh issue create --label improvement:gate-1` anyway.

```bash
npx @hoodiecollin/pm-playbook materialize --issue <n> --yes
```

That gives an `improvement` three sub-issues (design → plan → impl), a `bugfix`
two (diagnose → fix), and an `experiment` two (research → evaluate). It is
idempotent — it creates only what is missing.

## Step 3 — write the design into gate 1

Edit the gate issue's body. Keep it tight; a design is a decision aid, not a spec
dump:

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
The other options and why they're worse.

## Open questions
What's genuinely undecided and needs input before committing.
```

```bash
gh issue edit <gate-issue> --body-file <path>   # write to the scratchpad, never the repo tree
```

Write the body to the **scratchpad**, never into the repo tree — the whole point
is that it doesn't get committed.

## Lifecycle — what happens after the design

**Closing the gate means accepted.** That is the whole signalling mechanism;
there is no status label, and the commitment ladder is derived from which gates
are closed (`pm-playbook ladder`).

- **Accepted → proceed to gate 2 (the plan), then gate 3 (the build).** When the
  feature ships, distill the *durable* architecture — how the shipped thing
  works, the invariant it preserves — into `docs/ARCHITECTURE.md`. The design
  discussion stays in the closed gate; the tree gains a description of *what now
  exists*, not a plan for what might. (This is exactly how the multi-writer-MVCC
  proposal became the "control plane vs data plane" paragraph in ARCHITECTURE.md.)
- **Rejected / superseded.** Close the work item with a comment saying why,
  linking whatever replaced it. Nothing lands in the tree.

**Reopening an accepted gate? Purge the body FIRST** (CLAUDE.md → *Operating
disciplines* #7) — down to a placeholder saying the gate is being redone, before
any new thinking.

Never leave an accepted design as a committed file "for reference" — the
ARCHITECTURE.md distillation *is* the reference; the gate holds the rationale.

## Close with a note

Tell the user what you did: the work item and gate issue numbers + URLs, which
epic it sits under (or which existing item you used instead of opening a new
one), and — if you redirected something away from a committed file — say so, so
the discipline is visible rather than silent.
