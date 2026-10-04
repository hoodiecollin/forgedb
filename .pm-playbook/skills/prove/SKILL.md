---
name: prove
description: "Write an improvement's proof gate: the approach plus evidence for every claim it rests on — commands run, code read at a commit — then close it with pm-playbook prove or hand it to the user. Use when an improvement is at proof-next or proof-pending. Keywords - proof, gate:proof, claims, evidence, spike, probe, approach, design, plan, verify premise."
---

# Write the proof gate

The proof gate exists because approaches approved by reading kept turning out wrong. In one
project's history, more than half of the large improvements had an accepted design later disproved,
and the most common cause was a claim about how a library, compiler or runtime behaves that nobody
had run: a type assumed to be `Send`, a dylib assumed to survive being copied, a cache assumed to
save 19.7 s when the step took 40 ms. Reading the code would not have caught any of them. Running
something would have.

So this gate closes on evidence, not approval.

## Steps

1. **Read the intent gate** (closed — it is what you are proving an approach for) and the code the
   change touches. Note the commit you read it at: `git rev-parse --short HEAD`.

2. **Write the approach** in a few sentences under `### Approach`.

3. **List every claim the approach depends on** in the `### Claims` table. A claim belongs here if
   the approach would change were it false. Look hardest at:
   - how a third-party tool, library, compiler, OS or CI runner behaves;
   - any number — a timing, a size, a count;
   - facts about this codebase: where something is called, what a test exercises, what a config
     key does;
   - decisions in sibling issues this depends on (`npx @hoodiecollin/pm-playbook context <n>`).

4. **Prove each one.** The status column takes exactly one of:

   | Status | Evidence column holds |
   |---|---|
   | `ran` | the command, and its output or a CI link |
   | `read` | `path:line @ <commit>` |
   | `out-of-scope` | why it does not matter for this change |
   | `assumed` | nothing yet — this blocks closing |

   For behaviour claims, write the smallest probe that answers the question: a scratch test, a
   ten-line program, one command. Throwaway code goes on `spike/<issue>-<slug>`; cite the branch
   and commit under `### Spike`. It never merges.

   An honest `assumed` row is better than invented evidence. If a claim cannot be proven yet, leave
   it `assumed` and say what would prove it — or, if the whole approach hinges on it, propose
   filing an `experiment` first.

5. **Pre-mortem.** Assume this shipped and failed. Write down what broke. Each answer is either a
   claim you now add to the table, or a non-goal.

6. **One-way doors.** List choices that are expensive to reverse once shipped — a public API, an
   on-disk or wire format, a published package name. Write `None` if there are none.

7. **Get it disproved.** Hand the gate body to a fresh-context subagent with this brief: *"Try to
   show each claim in this table is false. For each, run the cheapest command or read the specific
   code that would disprove it, and report the command and output. Do not report style or
   completeness issues."* A fresh agent finds what the author's own review misses. Fix the table
   from what it finds.

8. **Close it, or hand it over.**

   ```bash
   npx @hoodiecollin/pm-playbook prove <gate-number>        # report
   npx @hoodiecollin/pm-playbook prove <gate-number> --yes  # close, if eligible
   ```

   `prove` closes the gate only when no claim is `assumed`, every other row has evidence, and the
   one-way doors are `None`. If it declines, it says why. When one-way doors are declared, the
   evidence is not the question — tell the user the gate is ready for their decision, and stop.

## If the build disproves a claim

Stop building on it. Say which claim failed and what showed it, and ask for the proof gate to be
reopened. Update the row in place — do not leave the old evidence with a note under it.
