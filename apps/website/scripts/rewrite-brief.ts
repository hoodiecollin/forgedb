/**
 * Print (and rewrite) the generation brief for one rewrite request:
 *
 *   bun scripts/rewrite-brief.ts <id>
 *
 * The dev route writes `.rewrite-queue/briefs/<id>.md` when a request lands; this
 * is the on-demand fallback — regenerate a brief that's missing or stale, or just
 * dump one to stdout. Prints the composed style + original slice + output spec.
 */
import { resolveDocFile } from "../lib/mdx";
import { readRequest } from "../lib/dev/rewrite-queue";
import { buildBrief, writeBrief } from "../lib/dev/rewrite-brief";

const id = process.argv[2];
if (!id) {
  console.error("usage: bun scripts/rewrite-brief.ts <id>");
  process.exit(2);
}

const req = readRequest(id);
if (!req) {
  console.error(`no request ${id} in .rewrite-queue/requests.jsonl`);
  process.exit(1);
}

const file = resolveDocFile(req.slug);
if (!file) {
  console.error(`no doc for slug ${req.slug.join("/")}`);
  process.exit(1);
}

const path = writeBrief(req, file); // refresh the on-disk brief
console.error(`# wrote ${path}\n`);
console.log(buildBrief(req, file));
