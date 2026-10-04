---
name: intent
description: "Write an improvement's intent gate with the user: the problem, the outcome, acceptance examples and non-goals, short enough to approve by reading. Use when an improvement is at intent-next or intent-pending. Keywords - intent, gate:intent, requirements, acceptance criteria, scope, non-goals, what and why."
---

# Write the intent gate

The intent gate answers one question a person has to decide: **is this the right thing to build,
and how will we know it is done?** It does not decide *how*. Approaches are proven in the proof
gate, because an approach approved by reading is exactly what slipped through before — claims about
how a tool or runtime behaves, accepted without anyone running anything.

## Steps

1. Find the gate: `npx @hoodiecollin/pm-playbook ladder --json`, then the `gate:intent` sub-issue of
   the work item. If there is none and the item is on the cycle in flight,
   `npx @hoodiecollin/pm-playbook materialize --yes`.

2. **Interview before drafting.** Ask the user what problem they are solving, for whom, what done
   looks like, and what is out of scope. Read the parent issue and any linked issues. Read the code
   the change would touch, so the problem statement matches the system as it is.

3. **Fill the seeded sections**, and keep the whole gate under about 400 words:
   - **Problem** — what is wrong or missing, and for whom.
   - **Outcome** — what is observably true when this is done.
   - **Acceptance examples** — Given / When / Then. These become the first failing tests in the
     build, so write them as things a test can check.
   - **Non-goals** — what this deliberately does not do.

4. Write the body by editing the gate issue (`gh issue edit <n> --body-file`), not in a comment. A
   body is what the next reader trusts.

5. **Stop and hand it to the user.** Say it is ready for review and what the main open questions
   are. You cannot close it; the user does, and that close is the approval.

## If it changes later

If the proof or the build shows the intent was wrong — the problem is different, an acceptance
example cannot be true — say so plainly and ask the user to reopen the gate. Replace the outdated
text in the body rather than adding a correction underneath it; a corrected paragraph left in place
reads as current.
