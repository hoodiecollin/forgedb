/**
 * Wake watcher for the in-browser prose-rewrite dev tool. Blocks until a request
 * needs Claude's attention — one that has neither a generated proposal nor a
 * recorded outcome — then prints its id(s) and exits 0. The Claude Code harness
 * re-invokes the agent on exit; the agent generates the proposal(s), then
 * re-launches this watcher to arm the next wake.
 *
 *   bun scripts/rewrite-watch.ts
 *
 * Event-driven (fs.watch) — no polling loop. Exits 0 with work, stays blocked
 * otherwise.
 */
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

/** Requests with no proposal file and no outcome — i.e. awaiting generation. */
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
    console.log(
      `  ${r.id}  [${r.target.kind}]  /${r.slug.join("/")}  —  ${JSON.stringify(r.instruction)}`,
    );
  }
  process.exit(0);
}

// Fire immediately if work is already queued.
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
