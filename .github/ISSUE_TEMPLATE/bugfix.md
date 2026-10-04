---
name: 🔴 Bugfix
about: A defect in behavior that already exists.
title: ""
labels: bugfix
---

<!--
  This body holds WHAT IS WRONG. A bugfix has no gates: the fix PR carries a regression test that
  fails before the fix and passes after, and `pm-playbook pr-check` fails a bugfix PR without one.

  Think this cannot wait for the next scheduled release? See the `fix` skill BEFORE adding
  `hotfix`: it must affect a released version, waiting must do real damage, and the fix must be
  bounded. A hotfix takes one gate, the warrant, which the maintainer closes.
-->

### In plain English
<!-- Two or three sentences: what this is about, for a reader who has never seen it.
     This is the FIRST section, always, and it is what other tooling reads (PLAYBOOK §8). -->

### What happens?

### What should happen instead?

### Where was it seen?
<!-- Version, environment, and whether an installed user of a PUBLISHED version can reach it.
     That last part is what decides whether the hotfix path is even available. -->

### Impact
<!-- What breaks for whom. Damage, not urgency-as-a-feeling. -->
