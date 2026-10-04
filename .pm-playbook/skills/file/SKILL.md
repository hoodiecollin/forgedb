---
name: file
description: "File a new issue under pm-playbook: check for duplicates, pick exactly one type, write the need in plain English, and link it to its epic. Use whenever new work is identified or the user asks to open an issue. Keywords - new issue, file, create, gh issue create, type label, epic, triage."
---

# File a work item

1. **Look for an existing issue first.** Read the mirror if it exists (`rg -il '<keywords>'
   .pm-playbook/backlog/`), otherwise `gh issue list --state all --search "<keywords>"`. Extend a
   close match rather than opening a second one — two issues for one need split its history.

2. **Pick exactly one type.**
   - `improvement` — makes the product better: a feature, refactor, performance work, debt.
   - `bugfix` — existing behaviour is wrong. Add `hotfix` only if it is in a *released* version and
     waiting for the next release does real damage (see the `fix` skill).
   - `experiment` — the deliverable is a finding, not shippable code. Never give it a milestone.

   If it seems to be two types, it is two issues.

3. **Write the body as the need, not the solution.** Open with `### In plain English` — two or
   three sentences for someone who has never seen it — then the detail. How it will be built
   belongs in the gates, which get written later against the code as it is then.

4. **Create it.** Write the body to a file and pass it with `--body-file`; bodies never travel as
   arguments.

   ```bash
   gh issue create --title "<what is wrong or missing>" --label improvement --body-file /tmp/body.md
   ```

   Leave it unmilestoned unless the user commits to it. An unmilestoned improvement is an *idea*,
   which is a real and useful state.

5. **Link it to its epic**, if one exists, as a native sub-issue (not a checkbox):

   ```bash
   id=$(gh api repos/OWNER/REPO/issues/<child> -q .id)
   gh api -X POST repos/OWNER/REPO/issues/<epic>/sub_issues -F sub_issue_id=$id
   ```

6. **Gates are not yours to create.** They appear when `pm-playbook materialize` runs for the
   cycle in flight (or `--issue <n>` for an experiment).
