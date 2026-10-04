---
name: next
description: "See where work stands and what comes next under pm-playbook: the derived ladder, what is left on a milestone, and the context pack an agent needs before working an issue or fanning out across several. Keywords - what's next, status, ladder, milestone, roadmap, what is left, context, neighbourhood, parallel agents, fan out."
---

# Where things stand, and what next

Refresh first if the mirror may be stale: `npx @hoodiecollin/pm-playbook pull`. It is gitignored
and machine-local, so a missing mirror means "not pulled here", never "no issues".

## What stage is each item at

```bash
npx @hoodiecollin/pm-playbook ladder            # every open work item and its rung
npx @hoodiecollin/pm-playbook ladder --json     # the same, for reading programmatically
```

The rung is computed from gate state; no label or filter carries it. Rungs: `idea` →
`intent-next/-pending` → `proof-next/-pending` → `build` for an improvement; `triage-next` → `fix`
for a bugfix (`warrant-next/-pending` first for a hotfix); `charter-*` → `verdict-*` for an
experiment.

## What is left on a release

Run `npx @hoodiecollin/pm-playbook milestone [vX.Y.Z]` (defaults to the cycle in flight). If it
exits 0, give its output as your entire reply, unchanged — it is built to be read as-is on a narrow
screen, and a retyped summary loses that. If it exits 2, relay the error and its remedy rather than
improvising an answer from other sources.

## Before working an issue, or briefing agents on several

```bash
npx @hoodiecollin/pm-playbook context <n>
```

This lists every neighbour of the issue — linked issues, epic siblings, shared milestone or surface
— and summarises the close ones. Read the whole roster. When dispatching agents, put this output in
each agent's brief rather than asking it to fetch context itself: an agent focused on its own task
skips optional reading, and the neighbour it skipped is the one it contradicts.

An agent that can see only part of a neighbourhood may draft intent, stating what it would disturb.
It should not build across issues it cannot see. Two agents proposing conflicting approaches for
coupled issues is a result to report to the user, not to resolve quietly.

## Choosing what to do next

Rank on scope, risk, and what unblocks what. Do not justify an order by demand or usage, and do not
estimate in time units. Report state; leave the choice to the user unless they ask for a
recommendation.
