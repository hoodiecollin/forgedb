---
name: check
description: "Lint the backlog against the pm-playbook invariants and fix what it finds, using the executable fix each violation carries. Use before finishing any task that touched issues, or when asked to audit tracking. Keywords - check, lint, invariants, violations, PM010, PM013, PM018, PM020, migrate."
---

# Check and fix the backlog

1. `npx @hoodiecollin/pm-playbook check --json`. Add `--no-remote` to lint the local mirror
   instead — the only tier that can read bodies, so the only one that runs PM017–PM019.

2. Exit 0 with nothing reported: say so in one line and stop.

3. Otherwise fix them, errors first. Every violation has a `fix`; read it before running it.
   Some need judgement rather than a command:
   - **PM013** (a gate set is incomplete) — run `materialize`; never create a gate by hand.
   - **PM018 / PM019** (a closed gate with an unproven claim, or empty) — the fix reopens a gate,
     which undoes a person's approval. Tell the user what is wrong and let them decide.
   - **PM020** (an open pre-4.0 gate) — closing it as not planned is safe: it is a stage of a model
     that no longer exists. Then `materialize` for the parent's 4.x gates.
   - **PM102** (a markdown backlog) — move live entries into issues before deleting the file, and
     check first that it is not generated output.
   - **PM103** (label migrations pending) — `npx @hoodiecollin/pm-playbook migrate` previews them.
     Show the user the plan; it rewrites shared labels, so do not apply it with `--yes` yourself.

4. Re-run `check` to confirm.

If a violation looks wrong, say why instead of working around it. A false positive is a linter bug,
and a local workaround hides it from everyone else.

PR-scoped rules run separately, usually in CI: `pr-check <pr>` (a bugfix PR changes a test) and
`scope-check <pr>` (no next-cycle work merged onto the integration branch).
