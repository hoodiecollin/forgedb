# Release engineering

Setting up or changing how a repo releases. An ordinary release needs only the `release` skill.

## Contents

1. The publish gap
2. One integration branch
3. How branches land
4. Where a check has to sit to block anything
5. Changelog and roadmap

## 1. The publish gap

The default branch must stay **releasable**, which is more than green. For a product whose built
output depends on packages the project itself publishes (a code generator and its runtime crates, a
plugin host and its SDK), work can land that makes the output require a newly published API.
In-tree everything resolves by path and the tests pass; an installed user, resolving from the
registry, cannot build. Only an outside-repo reclose sees this.

Pick one way to prevent it, and write the choice in `CONTRIBUTING.md`:

1. **Publish eagerly** — publish the new API before the dependent change lands. Costs many
   intermediate versions.
2. **Hold the gap off trunk** — `main` holds released state; core work integrates on `develop`,
   which may carry a gap. The release order is the point: **publish → merge `develop` into `main` →
   tag.** The reclose is a required check on `main`, not on `develop`, where it would be red all
   cycle and stop being read.

"We'll remember to publish before tagging" is not one of the options.

## 2. One integration branch

There is exactly one, and its name never contains a version. The registry has one version line per
package, so two cycle branches with unpublished changes cannot both be measured against it; and a
version in a branch name is a second copy of the schedule the milestone already holds.

`develop` means "the cycle in flight", so after a tag it simply is the next cycle. Work for a later
milestone waits on its own branch off `develop` and is rebased after the release. Two other
long-lived branches are legitimate: a maintenance line cut from a tag (`release/v0.4.x`) when an old
version needs a patch, and a track that cannot merge into the current cycle, named for the work
(`format-v2`).

The cycle in flight is derived, never configured: the lowest open core milestone whose `major.minor`
line has no closed milestone. That is why closing the milestone is part of the release ritual — it
is what advances the cycle.

## 3. How branches land

Pick squash, rebase or merge commit, and disable the other two in the repository settings so the
merge button and the docs cannot disagree. In `CONTRIBUTING.md`, record the choice, the exact local
command if work merges locally (`--no-ff` for merge commits, or a branch that is merely ahead
fast-forwards and loses its boundary), and that closing keywords only work on PRs into the default
branch.

The two directions between trunk and integration branch differ: integration → trunk *is* the
release and is a merge commit; trunk → integration is a sync and should be a rebase, or every
release leaves a content-free merge commit and `trunk..integration` stops meaning "unreleased".
Rebasing a pushed integration branch needs `--force-with-lease`; check branch protection first, and
confirm `git diff trunk integration` is empty before pushing a rebase that dropped commits.

## 4. Where a check has to sit to block anything

A check that cannot fail the thing it guards is a report. Two common shapes:

- A step deliberately written not to fail (`if: always()`), correct on every push, with no second
  invocation at the tag where it should fail.
- A check triggered by the same tag push as the release workflow. Two workflows on one event run in
  parallel; the check races the release instead of preceding it.

Put the check where the irreversible step is — a published version cannot be unpublished. Options:
fold it into the release tool's pre-release hook; make it a required status check; or accept
"parallel and loud" deliberately and write down that you did.

A claim that depends on things outside the repo — the registry, the runner's toolchain, an advisory
database, an expiring credential — changes without a commit. Give those checks a `schedule:` as
well as a push trigger. `pm-playbook check` belongs on a schedule too: PM013 becomes true or false
when a milestone closes, with no commit at all.

Publish steps should fail closed, and their retry should be idempotent: a publish that uploads
several artifacts can fail partway, and the naive retry then fails on the ones that already landed.

When release tooling generates part of your CI (cargo-dist, goreleaser, changesets), a custom job
that consumes its output depends on a file that rewrites itself. Record that coupling in a comment
at the coupling point.

## 5. Changelog and roadmap

Conventional commits → a generated changelog, rendered into both the GitHub Release and any website
changelog. Non-core `surface:*` work is filtered out of the core changelog and roadmap, and never
rides a core `v*` milestone (PM006). Trigger roadmap rebuilds on release *completion*, not tag push,
or a just-tagged version briefly shows as "next".
