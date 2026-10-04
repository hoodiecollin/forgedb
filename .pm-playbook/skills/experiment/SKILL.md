---
name: experiment
description: "Run an experiment under pm-playbook: a charter whose question can come back no, a fair method, a bounded scope, then a verdict that commits, kills, or is inconclusive. Use for spikes, benchmarks, evaluations and any work whose deliverable is a finding. Keywords - experiment, spike, benchmark, evaluate, research, charter, verdict, finding."
---

# Run an experiment

An experiment produces a finding, not shippable code. It never carries a milestone — you cannot
schedule a release around an answer you do not have yet. If the verdict commits work, that work is
filed as its own issue and milestoned.

Use one whenever a proof gate would otherwise rest on an `assumed` claim that is too big to settle
with a quick probe.

1. **Start it.** Gates for an experiment are created by decision:
   `npx @hoodiecollin/pm-playbook materialize --issue <n> --yes`.

2. **Charter** (`gate:charter`), written before any work:
   - the question, phrased so that "no" is a real possible answer;
   - the decision it informs — if no answer would change anything, do not run it;
   - the method, and what would make the comparison unfair (match durability, caching and
     transaction semantics across anything you benchmark);
   - the scope bound, in work (cases, files, candidates), not time;
   - what happens to any code: it lives on `spike/<issue>-<slug>` and never merges.

   Hand the charter to the user; they close it.

3. **Do the work** within the bound. Record commands and results as you go.

4. **Verdict** (`gate:verdict`): what was done, the answer, its limits — what it does *not*
   establish — and exactly one disposition: **commits** (link the issues you filed), **kills**
   (link what was closed as not planned), or **inconclusive** (say what would decide it). Hand it
   to the user; once they close it, close the experiment and delete the spike branch.
