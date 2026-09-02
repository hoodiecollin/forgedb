import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";
import { hashContent } from "./rewrite-hash";
import type { RewriteRequest, RewriteProposal } from "./rewrite-types";
function bodyAndBase(raw: string, absPath: string): { body: string; base: number } {
  if (absPath.endsWith(".mdx") || absPath.endsWith(".md")) {
    const { content } = matter(raw);
    const base = raw.indexOf(content);
    return { body: content, base: base < 0 ? 0 : base };
  }
  return { body: raw, base: 0 };
}

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
export function readProposals(ids: string[]): RewriteProposal[] {
  return ids.map(readProposal).filter((p): p is RewriteProposal => p !== null);
}
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
export function rejectProposal(id: string, ts: number): void {
  recordOutcome(id, "rejected", ts);
  const p = path.join(PROPOSALS_DIR, `${id}.json`);
  if (fs.existsSync(p)) fs.rmSync(p);
}
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
  const abs = path.resolve(proposal.srcFile);
  if (!abs.startsWith(CONTENT_DIR + path.sep)) {
    throw new Error(`refusing to write outside content/: ${abs}`);
  }

  const raw = fs.readFileSync(abs, "utf8");
  const { body, base } = bodyAndBase(raw, abs);
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
