# Agent instructions

<!-- pm-playbook:begin -->
## Project management — pm-playbook v4.0.0

Work is tracked in GitHub Issues. **Milestone = when**: assigning one means committed, and the
lowest open one is the cycle in flight. **Label = what kind**: every work item carries exactly one
of `improvement`, `bugfix`, `experiment`. Epics group work items as native sub-issues. There are
no priority or size fields.

| Type | Gates (sub-issues, `gate:<verb>`) | Then |
|---|---|---|
| `improvement` | intent → proof | build |
| `bugfix` | none — a `hotfix` takes a warrant | fix, with a regression test |
| `experiment` | charter → verdict (never milestoned) | — |

**A person closes a gate, not an agent.** Gates are created only by `pm-playbook materialize`. A
proof gate closes on evidence through `pm-playbook prove <n> --yes`; for any other gate that is
ready, say so and stop. If later work shows an accepted gate was wrong, say so and ask for it to be
reopened.

Load the skill for what you are doing (Claude Code: the `pm-playbook` plugin provides the same):

| When | Read |
|---|---|
| the model, and which skill to load | `.pm-playbook/skills/pm-playbook/SKILL.md` |
| filing a new issue | `.pm-playbook/skills/file/SKILL.md` |
| writing an improvement's intent gate | `.pm-playbook/skills/intent/SKILL.md` |
| writing or closing a proof gate | `.pm-playbook/skills/prove/SKILL.md` |
| implementing an improvement whose gates are closed | `.pm-playbook/skills/build/SKILL.md` |
| fixing a bug, or a hotfix | `.pm-playbook/skills/fix/SKILL.md` |
| a spike, benchmark or evaluation | `.pm-playbook/skills/experiment/SKILL.md` |
| what is left, what to do next, briefing parallel agents | `.pm-playbook/skills/next/SKILL.md` |
| tagging, the release-gate ledger, which branch to target | `.pm-playbook/skills/release/SKILL.md` |
| linting the backlog and fixing what it finds | `.pm-playbook/skills/check/SKILL.md` |

```bash
npx @hoodiecollin/pm-playbook pull     # refresh the local mirror at .pm-playbook/backlog/ (read it, edit via push)
npx @hoodiecollin/pm-playbook check    # before finishing — exit 0 means compliant
```
<!-- pm-playbook:end -->
