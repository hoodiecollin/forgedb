---
name: fix
description: "Fix a bug under pm-playbook: reproduce it as a failing test, fix it, and land one PR whose test proves it; for a hotfix, write the warrant gate first. Use for any bugfix or hotfix work item. Keywords - bug, bugfix, hotfix, regression test, reproduction, root cause, warrant, patch release."
---

# Fix a bug

A plain bugfix has no gates. Its decisions are proven rather than argued: a regression test that
fails before the fix and passes after shows the bug is real, that the fix addresses it, and keeps it
fixed. The PR is where that proof is reviewed, and `pm-playbook pr-check` fails a PR that closes a
bugfix without changing a test (PM021).

## Steps

1. **Reproduce it.** Exact inputs or steps against the current code. If you cannot reproduce it,
   say so in the issue rather than guessing at a fix.

2. **Write the failing test first**, from the reproduction. Run it and see it fail for the reason
   the bug describes — a test that fails for some other reason proves nothing.

3. **Find the root cause** and cite it in the issue or PR as `path:line`. The symptom is where it
   showed up; the cause is the mechanism.

4. **Fix it**, run the test to green, and run the rest of the suite.

5. **Open the PR** with `Closes #<n>`, the root cause, and the before/after test result.

## Hotfix

A hotfix is a bugfix in *released* behaviour that cannot wait for the next release. It takes one
gate, the **warrant**, because skipping the release queue is a decision for a person. All three
must hold, or it is an ordinary bugfix:

- it affects a released version;
- waiting does real damage — data loss, a security hole, a broken install, wrong output people act
  on (the test is the damage, never the wait);
- the fix is bounded: no public API, schema, config or dependency change, and one regression test
  covers it.

Open the patch milestone and assign it, then create the warrant gate — never by hand:
`npx @hoodiecollin/pm-playbook materialize --milestone vX.Y.Z --yes`. Fill it in (`gate:warrant`:
reproduction, damage, bound) and hand it to the user — they close it.

The fix then branches off `main`, lands and is tagged on its own patch milestone (`vX.Y.Z`,
holding only this item and its release-gate), and is merged forward into the integration branch.
The issue is not done until both branches carry it.
