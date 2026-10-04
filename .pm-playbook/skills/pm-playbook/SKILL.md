---
name: pm-playbook
description: "How this repo tracks work in GitHub Issues: milestones say when, one type label says what kind, and gates are sub-issues a human closes. Use for any issue-tracking task, then load the workflow skill it points to. Keywords - issue, backlog, milestone, label, epic, gate, intent, proof, hotfix, experiment, release."
---

# pm-playbook

Work is tracked in GitHub Issues with two axes and nothing else:

- **Milestone = when.** A milestone is a version (`v1.4.0`). Assigning one means committed; the
  lowest open milestone on an unreleased line is the *cycle in flight*, which is what "scheduled"
  means. There are no priority, size or area fields — a guessed number becomes a second schedule
  that drifts from the real one.
- **Label = what kind.** Every work item carries exactly one of `improvement`, `bugfix`,
  `experiment`. `epic` groups work items as native sub-issues; `release-gate` blocks a tag.

The tree is three levels: epic → work item → gate. Code and git history are ground truth; an issue
body is a claim about them, and when the two disagree the body is wrong.

## Gates

A gate is a sub-issue labelled `gate:<verb>`, created only by `pm-playbook materialize`. It exists
where a person has to decide something before work continues, and **a person closes it** — the
plugin's hook refuses an agent's `gh issue close` on a gate.

| Type | Gates | Then |
|---|---|---|
| `improvement` | `intent` → `proof` | build (the PR) |
| `bugfix` | none — `hotfix` adds `warrant` | fix (the PR carries a regression test) |
| `experiment` | `charter` → `verdict` | closes with the verdict |

The stage of each item is derived from its gates: `npx @hoodiecollin/pm-playbook ladder`.

## Which skill to load

| You are about to… | Load |
|---|---|
| file a new issue, or decide its type | `file` |
| write or revise an intent gate | `intent` |
| write a proof gate, or close one | `prove` |
| implement an improvement whose proof is closed | `build` |
| fix a bug, or handle a hotfix | `fix` |
| run a spike, benchmark or evaluation | `experiment` |
| see what is left, pick work, or brief parallel agents | `next` |
| tag a release, or touch the release-gate ledger | `release` |
| lint the backlog and fix what it finds | `check` |

## Always true

- Never create a gate by hand, and never close one yourself. A proof gate may close through
  `pm-playbook prove <n> --yes`, which checks its evidence first; every other gate waits for the
  maintainer. If a gate is ready, say so and stop.
- If later work shows an accepted gate was wrong, say so and ask for it to be reopened. Building on
  a premise you know is false is the failure the gates exist to prevent.
- No `TODO.md` or other markdown backlog — the backlog is Issues. When you commit to new work, file
  it first.
- Never estimate in time units; prioritize on scope, risk and what unblocks what.
- `npx @hoodiecollin/pm-playbook check` before finishing anything that touched issues.

If `.pm-playbook/` exists, it is the version this repo adopted and wins over this plugin. If it
does not, the repo has not adopted the playbook — say so before acting.
