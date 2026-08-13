# Agent instructions

<!-- pm-playbook:begin -->
## Project management — pm-playbook v2.0.0

Issue tracking in this repo follows the **pm-playbook** two-axis model. The full doctrine is
vendored at `.pm-playbook/` and is authoritative; this block is only a summary.

**Before you create, label, milestone, or close an issue — read `.pm-playbook/AGENT.md`.**
It is a short router: load only the reference section relevant to what you are doing.

**The two axes, and nothing else, organize work:**
- **Milestone** = *when*. Assigning one means **committed**. *Focus* — the milestone being the
  cycle in flight — is what means scheduled. There is no label for "committed but unscheduled."
- **Labels** = *what kind*. Epics decompose via **native sub-issues**, never checkboxes and never
  a Project field.
- There are **no Priority / Size / Workstream fields**. Do not propose adding any.

**Every work item carries exactly one type, and the type decides its gates:**

| Type | Gates |
|---|---|
| `improvement` | design → plan → impl |
| `bugfix` | diagnose → fix (`hotfix` is a bounded form of this) |
| `experiment` | research → evaluate (never milestoned) |

Each gate is a sub-issue labelled `{type}:gate-{n}`. A closed gate means approved. The tree is
exactly three levels: epic → work item → gate.

**The commitment ladder is DERIVED from gate state — there are no maturity labels.** Walk the
gates in order; the first not closed decides the rung. Ask for it with `pm-playbook ladder`; no
GitHub filter can compute it.

**Invariants — violating one is a bug, not a style preference:**
- Exactly **one** type label per work item — never zero, never two (PM010).
- `experiment` never carries a milestone. A spike's deliverable is a finding; it feeds the
  release spine, it never rides it (PM003).
- **Never create a gate by hand** — `pm-playbook materialize` owns them and creates a complete
  set at once. A hand-made gate destroys the meaning of an absent one.
- A gate's milestone equals its parent's (PM011); an `epic` never carries gates (PM012).
- `release-gate` always has a milestone and never carries `experiment`. An open `release-gate`
  means its milestone **cannot be tagged** (PM004/PM005).
- A non-core `surface:*` issue never rides a core `v*` milestone (PM006).

**Verify before opening a PR** — exit code 0 means compliant:

```bash
npx @hoodiecollin/pm-playbook check
```
<!-- pm-playbook:end -->
