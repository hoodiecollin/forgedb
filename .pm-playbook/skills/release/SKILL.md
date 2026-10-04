---
name: release
description: "Decide whether a milestone can be tagged and keep the release-gate asset ledger current; covers the release spine, patch milestones, and which branch a change targets. Use when preparing a release, landing a change that touches a published asset, or choosing a PR's base branch. Keywords - release, tag, release-gate, ledger, publish, milestone, patch, hotfix branch, develop, integration branch."
---

# Releases

A milestone is a version. Work closes *into* it when merged and reads "pending release" until the
GitHub Release for that version exists — closed and shipped are different states.

## Can this milestone be tagged?

```bash
npx @hoodiecollin/pm-playbook release-check vX.Y.Z
```

It reports two different blockers: open `release-gate` issues (release obligations — publishing,
version reconciliation, a credential rotation) and open work items (the milestone is incomplete).
An open `release-gate` blocks the tag even when every feature is closed.

A clean result is not proof the release works. If the project's built output depends on packages
it publishes itself, the only proof is an outside-repo reclose: from a clean directory, with the
*published* tool, run install → generate → build and confirm every dependency resolves from the
registry. Ask whether that was done. Do not tag or publish yourself; report and let the user run it.

## The ledger

Each milestone has a `release-gate` issue whose body is a table of **every** independently
versioned asset — every package, crate, extension, binary — each defaulting to "no change", created
when the milestone opens. When a change touches an asset, update its row in the same PR. An absent
row and a "no change" row look the same at tag time but mean opposite things: one was checked, the
other never considered. Include internal packages; their failure is the quiet one, a correct-looking
version number over stale source.

## Patch milestones

A patch milestone (`v1.2.1`) holds exactly one work item, its gates, and its release-gate (PM015).
A hotfix is the usual occupant; see the `fix` skill for when one is warranted.

## Which branch does a change target?

Ask: does it depend on, describe or demonstrate behaviour that is not released yet? If no — a typo,
a dependency bump, a fix to already-shipped docs — it goes to `main`. If yes, it goes to the
integration branch together with the feature, so documentation never ships ahead of what it
documents. `pm-playbook scope-check <pr>` refuses a PR into the integration branch that closes work
milestoned past the cycle in flight.

## Setting up release engineering

For the deeper mechanics — the publish gap, one integration branch, merge methods, where each
check has to sit to actually block a release — read
[reference/release-engineering.md](reference/release-engineering.md). Load it when setting up or
changing a repo's release process, not for an ordinary release.
