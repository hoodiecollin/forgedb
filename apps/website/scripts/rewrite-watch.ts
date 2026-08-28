import fs from "node:fs";
import path from "node:path";
import type { RewriteRequest } from "../lib/dev/rewrite-types";
const QUEUE_DIR = path.join(process.cwd(), ".rewrite-queue");
const REQUESTS = path.join(QUEUE_DIR, "requests.jsonl");
const PROPOSALS_DIR = path.join(QUEUE_DIR, "proposals");
const OUTCOMES = path.join(QUEUE_DIR, "outcomes.jsonl");
fs.mkdirSync(PROPOSALS_DIR, { recursive: true });
const readLines = (p: string): string[] =>
  fs.existsSync(p) ? fs.readFileSync(p, "utf8").split("\n").filter(Boolean) : [];

function unprocessed(): RewriteRequest[] {
  const requests = readLines(REQUESTS).map((l) => JSON.parse(l) as RewriteRequest);
  const done = new Set(
    readLines(OUTCOMES).map((l) => (JSON.parse(l) as { id: string }).id),
  );
  return requests.filter(
    (r) => !done.has(r.id) && !fs.existsSync(path.join(PROPOSALS_DIR, `${r.id}.json`)),
  );
}

function report(pending: RewriteRequest[]): never {
  console.log(`\n${pending.length} rewrite request(s) awaiting a proposal:`);
  for (const r of pending) {
    const brief = path.join(QUEUE_DIR, "briefs", `${r.id}.md`);
    const briefRef = fs.existsSync(brief)
      ? brief
      : `(missing — run: bun scripts/rewrite-brief.ts ${r.id})`;
    const where = r.contentKey ? `${r.contentModule}:${r.contentKey}` : `/${r.slug.join("/")}`;
    console.log(
      `  ${r.id}  [${r.target.kind}]  ${where}  —  ${JSON.stringify(r.instruction)}`,
    );
    console.log(`         brief: ${briefRef}`);
  }
  console.log(
    `\nRead each brief (it carries the composed style + original + output spec), ` +
      `then write .rewrite-queue/proposals/<id>.json.`,
  );
  process.exit(0);
}
const initial = unprocessed();
if (initial.length) report(initial);
console.log("watching .rewrite-queue for new rewrite requests… (ctrl-c to stop)");
const watcher = fs.watch(QUEUE_DIR, { recursive: true }, () => {
  const pending = unprocessed();
  if (pending.length) {
    watcher.close();
    report(pending);
  }
});
