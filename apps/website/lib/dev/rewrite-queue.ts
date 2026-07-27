/**
 * Filesystem-backed queue for the in-browser prose-rewrite dev tool. Server-only
 * (uses node:fs + gray-matter). Layout under `<app>/.rewrite-queue/`:
 *
 *   requests.jsonl        append-only log of RewriteRequest (one JSON per line)
 *   proposals/<id>.json   RewriteProposal, written by Claude, served to the overlay
 *   outcomes.jsonl        append-only log of {id, outcome, ts} for accept/reject
 *
 * The route handler is the only committed consumer that is gitignored; this
 * module stays committed so the logic survives a fresh checkout.
 */
import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";
import { hashContent } from "./rewrite-hash";
import type { RewriteRequest, RewriteProposal } from "./rewrite-types";

/**
 * The splice coordinate space for a file and where it begins in the raw bytes.
 *
 * - `.mdx`/`.md` docs: offsets are content-space (post-frontmatter), so the body
 *   is the frontmatter-stripped content and `base` is where it starts in `raw`.
 * - Any other file (e.g. a `.ts` content module): offsets are raw-file offsets,
 *   so the body is the whole file and `base` is 0. This keeps content-module
 *   splicing free of gray-matter's frontmatter handling entirely.
 */
function bodyAndBase(raw: string, absPath: string): { body: string; base: number } {
  if (absPath.endsWith(".mdx") || absPath.endsWith(".md")) {
    const { content } = matter(raw);
    const base = raw.indexOf(content);
    return { body: content, base: base < 0 ? 0 : base };
  }
  return { body: raw, base: 0 };
}

/** Content fingerprint of a file's splice body — for the staleness guard. */
export function contentHashOfFile(absPath: string): string {
  const raw = fs.readFileSync(absPath, "utf8");
  return hashContent(bodyAndBase(raw, absPath).body);
}

const QUEUE_DIR = path.join(process.cwd(), ".rewrite-queue");
const REQUESTS = path.join(QUEUE_DIR, "requests.jsonl");
const PROPOSALS_DIR = path.join(QUEUE_DIR, "proposals");
const OUTCOMES = path.join(QUEUE_DIR, "outcomes.jsonl");
const CONTENT_DIR = path.join(process.cwd(), "content");

function ensure(): void {
  fs.mkdirSync(PROPOSALS_DIR, { recursive: true });
}

/** Monotonic-ish id without Date.now (stable enough for a local dev queue). */
export function newId(existing: number): string {
  return `r${(existing + 1).toString().padStart(4, "0")}`;
}

export function readRequests(): RewriteRequest[] {
  if (!fs.existsSync(REQUESTS)) return [];
  return fs
    .readFileSync(REQUESTS, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((l) => JSON.parse(l) as RewriteRequest);
}

/** Append a request, assigning id + ts server-side. Returns the stored record. */
export function appendRequest(
  input: Omit<RewriteRequest, "id" | "ts" | "status">,
  ts: number,
): RewriteRequest {
  ensure();
  const req: RewriteRequest = {
    ...input,
    id: newId(readRequests().length),
    ts,
    status: "pending",
  };
  fs.appendFileSync(REQUESTS, JSON.stringify(req) + "\n");
  return req;
}

export function readRequest(id: string): RewriteRequest | null {
  return readRequests().find((r) => r.id === id) ?? null;
}

export function readProposal(id: string): RewriteProposal | null {
  const p = path.join(PROPOSALS_DIR, `${id}.json`);
  if (!fs.existsSync(p)) return null;
  return JSON.parse(fs.readFileSync(p, "utf8")) as RewriteProposal;
}

/** All proposals whose id is in `ids` and that exist on disk. */
export function readProposals(ids: string[]): RewriteProposal[] {
  return ids.map(readProposal).filter((p): p is RewriteProposal => p !== null);
}

/** Every proposal currently on disk (i.e. generated but not yet accepted/rejected). */
export function readAllProposals(): RewriteProposal[] {
  if (!fs.existsSync(PROPOSALS_DIR)) return [];
  return fs
    .readdirSync(PROPOSALS_DIR)
    .filter((f) => f.endsWith(".json"))
    .map((f) => JSON.parse(fs.readFileSync(path.join(PROPOSALS_DIR, f), "utf8")) as RewriteProposal);
}

function recordOutcome(id: string, outcome: string, ts: number): void {
  ensure();
  fs.appendFileSync(OUTCOMES, JSON.stringify({ id, outcome, ts }) + "\n");
}

/** Reject a proposal: log the outcome, drop the proposal file. */
export function rejectProposal(id: string, ts: number): void {
  recordOutcome(id, "rejected", ts);
  const p = path.join(PROPOSALS_DIR, `${id}.json`);
  if (fs.existsSync(p)) fs.rmSync(p);
}

/**
 * Apply candidate `index` of proposal `id`: splice its text into the backing
 * `.mdx` at the proposal's authoritative [srcStart, srcEnd] (content-space),
 * translating to raw-file coordinates via the frontmatter prefix. Returns the
 * absolute file path written.
 */
export function acceptProposal(
  id: string,
  index: number,
  ts: number,
  expectedHash?: string,
): string {
  const proposal = readProposal(id);
  if (!proposal) throw new Error(`no proposal ${id}`);
  const candidate = proposal.candidates[index];
  if (!candidate) throw new Error(`no candidate ${index} in ${id}`);

  // Path safety: the proposal's srcFile must resolve inside content/.
  const abs = path.resolve(proposal.srcFile);
  if (!abs.startsWith(CONTENT_DIR + path.sep)) {
    throw new Error(`refusing to write outside content/: ${abs}`);
  }

  const raw = fs.readFileSync(abs, "utf8");
  const { body, base } = bodyAndBase(raw, abs);

  // Staleness backstop: if the body changed since the browser rendered it, the
  // offsets are stale and splicing would corrupt the file. Refuse.
  if (expectedHash && hashContent(body) !== expectedHash) {
    throw new Error("STALE: source changed since render — reload the page and retry");
  }

  const start = base + proposal.srcStart;
  const end = base + proposal.srcEnd;
  const next = raw.slice(0, start) + candidate.text + raw.slice(end);
  fs.writeFileSync(abs, next);

  recordOutcome(id, `accepted:${index}`, ts);
  fs.rmSync(path.join(PROPOSALS_DIR, `${id}.json`));
  return abs;
}
