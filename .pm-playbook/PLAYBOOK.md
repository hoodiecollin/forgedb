# pm-playbook — how work is tracked, and why

GitHub Issues are the backlog. Two axes organise them — milestones say *when*, labels say *what
kind* — and gates record the decisions a person has to make before work continues. A linter, a CLI
and a hook enforce the rules; workflow skills tell an agent how to follow them.

This document explains the model and the reasons behind it, for the people who adopt it. Agents
work from the skills (`skills/`), which are short and task-shaped; they do not need to read this
end to end.

> **Code and git history are ground truth.** An issue body, a label, a roadmap page, a memory note —
> each is a claim about them. When a claim disagrees with the code, the claim is wrong.

---

## 1. The model

| Axis | Mechanism | Answers |
|---|---|---|
| **When** | Milestone = a version (`v1.4.0`) | Are we committed to this, and for which release? |
| **What kind** | Labels | What kind of work is this, and so which gates does it take? |

- **A milestone means committed.** The *cycle in flight* — the lowest open milestone on a release
  line that has not shipped — is what means scheduled. It is derived, never configured, so it
  advances by itself when a milestone closes.
- **Epics** group work items as GitHub native sub-issues, never task-list checkboxes. A work item
  holds its **gates**, also as sub-issues. That is the whole tree: epic → work item → gate.
- **The Project board is a view** over issues, never a second record.
- **No priority, size, effort or area fields.** Each is a second way of slicing the work, a second
  schedule that drifts from the milestone, and a guessed number steering scope. If GitHub issue
  fields such as Priority or Effort are enabled for your organisation, leave them unused.

Everything else — the stage an item is at, the roadmap, "what is left in this release" — is
computed from these, so there is nothing to keep in sync by hand.

## 2. Gates

### What a gate is for

A gate exists only where **a person has to decide something an agent cannot, and deciding it first
is cheaper than deciding it after.** Anything else is evidence, which the agent produces, or status,
which the tool derives.

That rule came from evidence. Under the previous model (design → plan → impl for every
improvement, diagnose → fix for every bug), a review of the main project using it found two
failures. On large work, gates that were genuinely approved in sequence still let wrong premises
through — 8 of 17 improvements that reached implementation had an accepted gate later shown wrong,
most often a claim about how a library, compiler or runtime behaves that nobody had run. Reading the
code could not have caught those; running something would have. On small work, gates were closed in
a batch after the fact — 23 of 42 completed items had every gate closed within half an hour — so
they cost effort and caught nothing.

### The gate sets

| Type | Gates | After the gates |
|---|---|---|
| `improvement` | **intent** → **proof** | `build`: the PR |
| `bugfix` | none (a `hotfix` takes **warrant**) | `fix`: the PR, with a regression test |
| `experiment` | **charter** → **verdict** | the verdict closes it |

- **Intent** is the decision only a person can make: is this the right thing to build, and how will
  we know it is done? Problem, outcome, acceptance examples, non-goals — short enough to approve by
  reading. It does not decide *how*.
- **Proof** is how, and it closes on evidence rather than approval. Its body holds the approach and
  a **claims table**: every claim the approach depends on, each marked `ran` (a command and its
  output), `read` (`path:line @ commit`), `out-of-scope` (with the reason), or `assumed`. An
  `assumed` row blocks closing. It also lists **one-way doors** — choices expensive to reverse once
  shipped. Throwaway probe code is allowed on a `spike/` branch and never merges.
- **Warrant** is the one bugfix decision a person has to make: may this skip the release queue? A
  plain bugfix needs no gate; a regression test that fails before the fix and passes after proves
  more than a diagnosis document, and the PR is where it is reviewed.
- **Charter** and **verdict** frame and conclude an experiment (§4).

### Who closes a gate

**A person.** A closed gate means someone decided, and an agent works on that person's GitHub token,
so GitHub cannot tell the two apart. The plugin's hook therefore refuses an agent's `gh issue close`
on a gate, and `push` refuses to close one through the local mirror.

The exception is a proof gate whose claims are all proven and which declares no one-way doors:
there is nothing left but evidence, which a tool checks as well as a person. `pm-playbook prove <n>
--yes` closes it in that case and refuses otherwise.

### Going back

When later work shows an accepted gate was wrong, the gate is reopened and the body corrected in
place. An agent told to treat accepted decisions as settled will build on a premise it can see is
false, so the skills tell it the opposite: stop, say which claim failed and what showed it, and ask
for the gate to be reopened.

### Mechanics

- Gates are created only by `pm-playbook materialize`, as a complete set: for every improvement and
  hotfix on the cycle in flight or an open patch milestone, or for one experiment with `--issue`.
  If people could create gates, an absent gate could mean "not created yet" or "nobody wrote it",
  and PM013 — every item in flight carries its full set — would mean nothing.
- A gate carries its parent's milestone (PM011). An epic never carries gates (PM012).
- The **ladder** — the stage an item is at — is derived by walking its gates in order: the first
  that is absent is `<verb>-next`, the first that is open is `<verb>-pending`. Ask for it with
  `pm-playbook ladder`; no label holds it, so no label can be stale.
- A closed proof gate must have a sound claims table (PM018), and no gate may be closed empty
  (PM019), while its parent is open.
- Gates from before 4.0 are relabelled `gate:retired` by `migrate`. A closed one is history; an open
  one is a stage of a model that no longer exists (PM020).

## 3. Labels

Every work item carries **exactly one** type (PM010):

| Label | Means |
|---|---|
| `improvement` | Makes the product better: features, refactors, performance, debt. |
| `bugfix` | Existing behaviour is wrong. A PR closing one must change a test (PM021). |
| `experiment` | The deliverable is a finding, not shippable code. Never milestoned (PM003). |

Plus `hotfix` (a bugfix on a released version that cannot wait — always with `bugfix`, always on a
patch milestone, PM014), `epic` (a container, not a work item), `release-gate` (blocks a tag), the
five gate labels `gate:intent`, `gate:proof`, `gate:warrant`, `gate:charter`, `gate:verdict`, and
`gate:retired` for history.

A label's description *is* its process — `bootstrap` writes them, so the GitHub UI explains each
one. There are deliberately no flavour labels (`perf`, `tech-debt`), no status labels (`has-design`,
`plan-next`), and no GitHub stock labels: flavour is what the body is for, status is derived, and
"closed as not planned" replaces `wontfix`.

`surface:*` labels name independently shipped products in a repo that ships more than one (§6).

## 4. Experiments

An experiment's deliverable is a finding, so it never rides a release: you cannot schedule a
feature on an answer you do not have yet (PM003). Its verdict may **commit** work — filed as its own
issue and milestoned — **kill** it, or be **inconclusive**, saying what would decide it.

The charter states a question that can come back "no", the decision it informs, a fair method, a
scope bound in work rather than time, and what happens to the code (it never merges). An
experiment whose verdict is closed is finished (PM016).

Use one whenever a proof gate would rest on an `assumed` claim too large for a quick probe.

## 5. Milestones and releases

- A milestone is a version, never a theme or a sprint. Keep a short spine of open milestones ahead
  of the current one; "1.0" is a horizon until its contents are real.
- **Closed is not shipped.** Work closes into a milestone when it merges and reads "pending release"
  until the GitHub Release for that version exists.
- An open **`release-gate`** issue means its milestone cannot be tagged, however complete the
  features are (PM004, PM005). Each milestone's release-gate carries a **ledger** of every
  independently versioned asset, each row defaulting to "no change", updated in the same PR as any
  change that touches the asset.
- A **patch milestone** (`v1.2.1`) holds exactly one work item and its release-gate (PM015), so a
  patch release stays bounded.
- A PR into the integration branch may not close work milestoned past the cycle in flight (PM008).

The release mechanics — the publish gap, one integration branch, merge methods, where a check has
to sit to block anything — are in the `release` skill's reference file.

## 6. Surfaces

A repo that ships more than one product labels each non-core one `surface:<name>`. Non-core
surfaces ship on their own line and their own milestones (`ext-v0.1.0`), never a core `v*`
milestone (PM006), and are filtered out of the core changelog and roadmap.

## 7. Epics and the roadmap

An epic is an umbrella that may span releases; each child carries its own milestone. Children are
native sub-issues (PM007), and only an epic has non-gate children (PM105). A relation between two
issues that neither can carry — "these ship together" — belongs on their epic.

The roadmap is computed from milestones, labels and sub-issues: **shipped** (released),
**active** (on the cycle in flight), **committed** (a later milestone), **labs** (experiments),
**ideas** (no milestone, no gates). Gates are status, not roadmap rows.

## 8. Writing issues

- **The backlog is Issues.** No `TODO.md` (PM102). When you commit to work, file it first.
- **Every work item and epic opens with `### In plain English`** — two or three sentences for a
  reader who has never seen it (PM017).
- **A body states current truth only.** When something is superseded, replace it in the same edit;
  do not strike it through or correct it underneath. A superseded paragraph reads as current,
  because that is what a body is.
- **Reasoning is proportionate to the decision.** A gate that takes ten thousand words to approve
  is not approved, it is skimmed.
- **Design lives in gates, not committed files.** When a feature ships, its durable architecture
  goes into `ARCHITECTURE.md`.
- **Read the local mirror** (`pm-playbook pull` → `.pm-playbook/backlog/`) instead of one API call
  per question. It is gitignored, goes stale when anyone else moves an issue, and is written back
  only through `push`, which refuses when both sides moved.

## 9. Enforcement

A rule that nothing can fail is a suggestion. Each rule here names where it fails:

| Where | What it enforces |
|---|---|
| `pm-playbook check` (CI on PRs and on a schedule) | every PM rule over issues; PM017–PM019 need the mirror (`--no-remote`) |
| `pm-playbook pr-check <pr>` (CI on PRs) | PM021: a bugfix PR changes a test |
| `pm-playbook scope-check <pr>` (CI on PRs to the integration branch) | PM008 |
| `pm-playbook release-check <vX.Y.Z>` (before the tag, in front of the release) | open release-gates and open work |
| The plugin's PreToolUse hook | no gate closed by an agent; no gate label or contradictory labels set by hand |
| `push` | no gate closed through the mirror; no edit that would create a violation |
| `prove` | a proof gate closes only on complete evidence and no one-way doors |

`check` needs a schedule as well as a PR trigger: PM013 becomes false the moment a milestone
closes, with no commit at all. `pm-playbook rules` prints every rule.

## 10. Adopting and upgrading

1. `npx @hoodiecollin/pm-playbook init` — vendors this document and the skills into `.pm-playbook/`
   and adds a short stanza to `AGENTS.md`. Commit what it writes. Claude Code users can install the
   plugin instead of, or as well as, the vendored skills.
2. `npx @hoodiecollin/pm-playbook bootstrap --repo <owner>/<name>` — labels with descriptions, a
   starter milestone, and the Project views.
3. Give every open issue exactly one type; `check --all-states` lists the rest.
4. `materialize --yes` for the cycle in flight.
5. Wire `check`, `pr-check`, `scope-check` and `release-check` into CI (§9). Write the repo's
   publish strategy and merge method into `CONTRIBUTING.md` (see the release reference).

**From 3.x:** `npx @hoodiecollin/pm-playbook@4 init`, then `migrate` (preview, then `--yes`). It
renames design gates to `gate:intent`, keeps experiment gates as `gate:charter`/`gate:verdict`, folds
every other old gate onto `gate:retired`, and rewrites every label description. Then close each open
`gate:retired` as not planned (PM020 lists them) and run `materialize --yes`: improvements past
design now owe a proof gate, and hotfixes a warrant.
