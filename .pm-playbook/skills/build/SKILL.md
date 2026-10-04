---
name: build
description: "Implement an improvement whose intent and proof gates are closed: riskiest path first as a thin end-to-end slice, acceptance examples as failing tests, one PR that closes the work item. Use when an improvement is at the build rung. Keywords - implement, build, PR, tracer bullet, TDD, acceptance tests, close issue."
---

# Build an improvement

Check the rung first: `npx @hoodiecollin/pm-playbook ladder --json`. Build only from `build` — both
gates closed. If either is open, the work is not yet agreed or not yet proven.

1. **Branch** from the branch this repo integrates into (read `CONTRIBUTING.md`; work for an
   unreleased feature usually targets the integration branch, not `main`).

2. **Riskiest path first.** Build a thin slice that runs end to end through the part of the proof
   most likely to be wrong, before filling in the rest. It surfaces a bad premise while it is still
   cheap to change.

3. **Acceptance examples become failing tests.** Write the intent gate's Given / When / Then as
   tests, see them fail, then implement until they pass.

4. **When a proven claim turns out false**, stop. Report the claim, the evidence that broke it, and
   what it changes, and ask for the proof gate to be reopened. Do not carry on and record it as a
   "deviation" — that is how a known-wrong premise ships.

5. **The plan lives in the PR.** Describe what changed and why in the PR body, link the gates, and
   use a closing keyword for the work item (`Closes #<n>`). Note that GitHub honours closing
   keywords only on PRs into the default branch; on an integration branch, close the item by hand
   after the merge and say what you verified.

6. Run the project's tests and `npx @hoodiecollin/pm-playbook check` before asking for review.
