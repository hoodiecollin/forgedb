#!/usr/bin/env bun
/**
 * Cycle-scope gate — "does this work belong in the release cycle currently in flight?"
 *
 * ForgeDB runs two long-lived branches (CLAUDE.md → the branch model): `main` holds released
 * state, `develop` holds the cycle in flight and is allowed to carry the publish gap. There is
 * exactly ONE integration branch and its name never contains a version, so the thing that keeps
 * next-cycle work off it cannot be the branch name. It is the milestone.
 *
 *   > Work landing on `develop` may not close an issue milestoned later than the cycle in flight.
 *
 * Note the shape: a DENY-LIST on future milestones, not an allow-list on the current one. That is
 * what lets it run with no escape hatches — an untracked chore, a CI fix, or a typo PR closes
 * nothing and passes silently, and correctly so. Work with no issue cannot be next-cycle work,
 * because next-cycle work is *defined* by carrying that milestone.
 *
 * The cycle in flight is DERIVED — the lowest open `v*` milestone on an unreleased line — never
 * configured. Nothing to update, nothing to drift from the actual spine, and it advances on its
 * own the moment a milestone closes. That is also its one prerequisite: closing the milestone is
 * part of the release ritual. A milestone left open after its tag freezes the gate and starts
 * blocking legitimate next-cycle work — loudly, which is the right direction to fail in.
 *
 * TWO INPUT MODES, because this repo works both ways:
 *
 *   --pr <n>       Read the PR's closing-issue links (GitHub's own linkage). This is what CI runs.
 *   --issue <n,…>  Check issues directly. This repo merges most branches locally under the
 *                  auto-merge rhythm, so a PR-only gate would rarely fire; run this before
 *                  merging a branch back into `develop`.
 *
 * Portable form: PLAYBOOK §5.3, rules PM008/PM009 in the `ai-pm-playbook` package. Once that
 * package is published to npm this file should be replaced by:
 *
 *   npx ai-pm-playbook scope-check <pr>
 *
 * Keep the two in sync until then — the playbook is canonical.
 */

const REPO = "hoodiecollin/forgedb";

interface IssueRef {
  number: number;
  title: string;
  url: string;
  milestone: string | null;
}

async function gh(args: string[]): Promise<string> {
  const proc = Bun.spawn(["gh", ...args], { stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (code !== 0) throw new Error(`gh ${args.join(" ")} exited ${code}\n${err.trim() || out.trim()}`);
  return out;
}

/** Numeric components of a `vX.Y.Z` title, or null when it is not on the core spine. */
function parseVersion(title: string): number[] | null {
  const m = /^v(\d+(?:\.\d+)*)/i.exec(title.trim());
  return m ? m[1]!.split(".").map(Number) : null;
}

/** Version order, NOT lexical order — `v0.10.0` must sort after `v0.9.0`. */
function compareMilestones(a: string, b: string): number {
  const va = parseVersion(a) ?? [];
  const vb = parseVersion(b) ?? [];
  for (let i = 0; i < Math.max(va.length, vb.length); i++) {
    const d = (va[i] ?? 0) - (vb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * The cycle in flight: the lowest open core milestone **on an unreleased line**.
 *
 * `^v\d` deliberately excludes the non-core surface namespaces (`vscode-v*`) — those ship on their
 * own line and are not on this spine, so "later than the cycle" is not a question with an answer
 * for them.
 *
 * The "on an unreleased line" clause is what stops a HOTFIX from hijacking the gate. A patch
 * milestone (v0.4.1) sorts below the real cycle (v0.5.0), so taking the lowest open milestone
 * outright makes the patch the cycle — and then every legitimate v0.5.0 PR into `develop` is
 * "milestoned later than the cycle in flight" and gets blocked for the entire hotfix window. A
 * CLOSED milestone on the same `major.minor` line is the evidence that line already shipped, so
 * the whole line is skipped rather than just that one tag. PLAYBOOK §5.6, and the same rule
 * `pm-playbook`'s own `currentCycle` applies.
 */
async function currentCycle(): Promise<string | null> {
  const [openRaw, closedRaw] = await Promise.all([
    gh(["api", `repos/${REPO}/milestones?state=open&per_page=100`]),
    gh(["api", `repos/${REPO}/milestones?state=closed&per_page=100`]),
  ]);

  const coreTitles = (raw: string): string[] =>
    (JSON.parse(raw) as { title: string }[]).map((m) => m.title).filter((t) => /^v\d/i.test(t.trim()));

  const line = (title: string): string => {
    const m = /^v(\d+)\.(\d+)\./i.exec(title.trim());
    return m ? `${m[1]}.${m[2]}` : title.trim();
  };

  const shippedLines = new Set(coreTitles(closedRaw).map(line));

  const open = coreTitles(openRaw)
    .filter((t) => !shippedLines.has(line(t)))
    .sort(compareMilestones);
  return open[0] ?? null;
}

async function issuesFromPr(pr: number): Promise<{ base: string; closing: IssueRef[]; mentioned: number[] }> {
  const query = `
    query($owner:String!,$name:String!,$pr:Int!){
      repository(owner:$owner,name:$name){
        pullRequest(number:$pr){
          body title baseRefName
          closingIssuesReferences(first:100){ nodes{ number title url milestone{ title } } }
        }
      }
    }`;
  const [owner, name] = REPO.split("/");
  const out = await gh([
    "api", "graphql", "-f", `query=${query}`,
    "-F", `owner=${owner}`, "-F", `name=${name}`, "-F", `pr=${pr}`,
  ]);
  const node = JSON.parse(out)?.data?.repository?.pullRequest;
  if (!node) throw new Error(`${REPO}#${pr} is not a pull request (or is not visible).`);

  const closing: IssueRef[] = (node.closingIssuesReferences?.nodes ?? []).map(
    (n: { number: number; title: string; url: string; milestone: { title: string } | null }) => ({
      number: n.number, title: n.title, url: n.url, milestone: n.milestone?.title ?? null,
    }),
  );
  const closingNumbers = new Set(closing.map((c) => c.number));
  const mentioned = [
    ...new Set([...`${node.title ?? ""}\n${node.body ?? ""}`.matchAll(/#(\d+)\b/g)].map((m) => Number(m[1]))),
  ].filter((n) => !closingNumbers.has(n));

  return { base: node.baseRefName, closing, mentioned };
}

async function issueRefs(numbers: number[]): Promise<IssueRef[]> {
  const out: IssueRef[] = [];
  for (const n of numbers) {
    const raw = JSON.parse(await gh(["issue", "view", String(n), "--repo", REPO, "--json", "number,title,url,milestone"]));
    out.push({ number: raw.number, title: raw.title, url: raw.url, milestone: raw.milestone?.title ?? null });
  }
  return out;
}

function usage(): never {
  console.error(
    "usage: bun scripts/check-cycle-scope.ts (--pr <n> | --issue <n[,n...]>) [--integration-branch <name>]\n" +
      "\n" +
      "  --pr <n>       check a pull request's closing links (what CI runs)\n" +
      "  --issue <n,…>  check issues directly, before a local merge into develop\n",
  );
  process.exit(2);
}

const argv = process.argv.slice(2);
function flag(name: string): string | undefined {
  const i = argv.indexOf(`--${name}`);
  if (i !== -1 && argv[i + 1] && !argv[i + 1]!.startsWith("--")) return argv[i + 1];
  const eq = argv.find((a) => a.startsWith(`--${name}=`));
  return eq ? eq.slice(name.length + 3) : undefined;
}

const prFlag = flag("pr");
const issueFlag = flag("issue");
const integration = flag("integration-branch") ?? "develop";
if (!prFlag && !issueFlag) usage();

const inFlight = await currentCycle();
if (inFlight === null) {
  // A legitimate state between releases, but worth saying out loud: mid-cycle it means the spine
  // is empty when it should not be.
  console.log("! No open v* milestone — no cycle in flight, so there is nothing to gate against.");
  process.exit(0);
}
const cycle: string = inFlight;

let closing: IssueRef[];
let mentioned: number[] = [];
let subject: string;

if (prFlag) {
  const pr = Number(prFlag);
  if (!Number.isInteger(pr) || pr <= 0) usage();
  const info = await issuesFromPr(pr);
  if (info.base !== integration) {
    console.log(`✓ PR #${pr} targets \`${info.base}\`, not \`${integration}\` — the cycle-scope gate does not apply.`);
    process.exit(0);
  }
  ({ closing, mentioned } = info);
  subject = `PR #${pr} → ${integration}`;
} else {
  const numbers = issueFlag!.split(",").map((s) => Number(s.trim())).filter((n) => Number.isInteger(n) && n > 0);
  if (!numbers.length) usage();
  closing = await issueRefs(numbers);
  subject = `issue(s) ${numbers.map((n) => `#${n}`).join(", ")}`;
}

const isFuture = (m: string | null) => m !== null && /^v\d/i.test(m.trim()) && compareMilestones(m, cycle) > 0;

console.log(`Cycle-scope gate — ${subject}`);
console.log(`Cycle in flight: ${cycle} (derived: lowest open v* milestone on an unreleased line)\n`);

const blocked = closing.filter((c) => isFuture(c.milestone));

// Advisory tier: a bare `#N` is usually context ("relates to", "part of"), so it never fails the
// run — but a mention is sometimes a closing keyword someone forgot to write, and that is the one
// way real next-cycle work slips past the strict tier.
const warned: IssueRef[] = mentioned.length ? (await issueRefs(mentioned)).filter((r) => isFuture(r.milestone)) : [];

for (const c of blocked) {
  console.log(`✗ PM008  #${c.number} ${c.title}`);
  console.log(`    Milestoned \`${c.milestone}\`, later than \`${cycle}\`. Merging this would land`);
  console.log(`    next-cycle work on \`${integration}\`.`);
  console.log(`    fix: keep it on its own branch until ${cycle} ships, then rebase and land it.`);
  console.log(`         If it genuinely belongs in ${cycle}: gh issue edit ${c.number} --milestone ${cycle}`);
  console.log(`    ${c.url}\n`);
}

for (const w of warned) {
  console.log(`! PM009  #${w.number} ${w.title}`);
  console.log(`    Referenced but not closed; milestoned \`${w.milestone}\`, later than \`${cycle}\`.`);
  console.log(`    Advisory — confirm a closing keyword was not simply left off.`);
  console.log(`    ${w.url}\n`);
}

if (blocked.length) {
  console.log(`${blocked.length} error(s), ${warned.length} warning(s).`);
  process.exit(1);
}

const summary = closing.length
  ? closing.map((c) => `#${c.number} (${c.milestone ?? "no milestone"})`).join(", ")
  : "nothing — no closing link, so this cannot be landing scheduled work";
console.log(`✓ Closes ${summary} — none milestoned past ${cycle}.`);
if (warned.length) console.log(`${warned.length} warning(s); warnings do not fail the run.`);
process.exit(0);

export {};
