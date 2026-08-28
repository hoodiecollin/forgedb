#!/usr/bin/env bun
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
function parseVersion(title: string): number[] | null {
  const m = /^v(\d+(?:\.\d+)*)/i.exec(title.trim());
  return m ? m[1]!.split(".").map(Number) : null;
}
function compareMilestones(a: string, b: string): number {
  const va = parseVersion(a) ?? [];
  const vb = parseVersion(b) ?? [];
  for (let i = 0; i < Math.max(va.length, vb.length); i++) {
    const d = (va[i] ?? 0) - (vb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
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
