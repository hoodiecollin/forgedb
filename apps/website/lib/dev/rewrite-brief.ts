/**
 * Assembles the generation brief for one rewrite request — everything the
 * generator (Claude, this session) needs in one place, so nothing is re-derived
 * or forgotten. Written to `.rewrite-queue/briefs/<id>.md` the moment a request
 * is enqueued; the wake watcher points at it.
 *
 * The brief bundles: the request context, the exact source slice to rewrite, the
 * composed style (spine + the target's register), grounding instructions, and the
 * required proposal shape. Server-only (node:fs + gray-matter).
 */
import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";
import { composeStyle, registerFile, type Tier } from "./rewrite-style";
import type { RewriteRequest } from "./rewrite-types";

const QUEUE_DIR = path.join(process.cwd(), ".rewrite-queue");
const BRIEFS_DIR = path.join(QUEUE_DIR, "briefs");

/** Path to a request's brief file (may not exist yet). */
export function briefPath(id: string): string {
  return path.join(BRIEFS_DIR, `${id}.md`);
}

/** Shared grounding + style + output-spec tail, given the resolved style tier. */
function briefTail(req: RewriteRequest, file: string, tier: Tier): string {
  const nCandidates = req.mode === "candidates" ? "3 candidates" : "1 candidate";
  return `## Grounding — verify before you write

Every claim must match ground truth (the code + git history + runtime), not just the
prose around it. A rewrite that makes a limit disappear or overstates a guarantee is a
regression, however smooth it reads.

- Backing file: \`${file}\`
- Code truth: this repo — \`CLAUDE.md\`, \`crates/\`, \`examples/*.forge\`. Never assert a
  behavior, limit, or number the code contradicts.

## Style — FOLLOW THIS (spine + \`${registerFile(tier)}\`)

${composeStyle(tier)}
## Output

Write \`.rewrite-queue/proposals/${req.id}.json\` — a RewriteProposal:

\`\`\`jsonc
{
  "id": "${req.id}",
  "srcFile": ${JSON.stringify(file)},
  "srcStart": ${req.target.srcStart},   // authoritative; splice exactly this range
  "srcEnd": ${req.target.srcEnd},
  "original": <the current source at [srcStart, srcEnd]>,
  "candidates": [ /* ${nCandidates}: { "text": <replacement>, "note"?: <one-line diff> } */ ],
  "mode": ${JSON.stringify(req.mode)}
}
\`\`\`
`;
}

/**
 * Brief for a content-key target: the value lives inside a `dd`` tagged template
 * in a typed TS content module and is rendered as Markdown/MDX, so the shape and
 * constraints differ from an `.mdx` doc block.
 */
function buildContentBrief(req: RewriteRequest, file: string): string {
  const raw = fs.readFileSync(file, "utf8");
  const original = raw.slice(req.target.srcStart, req.target.srcEnd);
  const nCandidates = req.mode === "candidates" ? "3 candidates" : "1 candidate";

  return `# Rewrite brief — ${req.id} (content module)

- **Content module:** \`${req.contentModule}\` — \`${file}\`
- **Slot:** \`${req.contentKey}\`
- **Target:** content · raw offsets [${req.target.srcStart}, ${req.target.srcEnd}]
- **Mode:** ${req.mode} → produce ${nCandidates}
- **Instruction:** ${JSON.stringify(req.instruction)}

> ⚠ **This is the body of a \`dd\\\`…\\\`\` tagged-template literal in a TypeScript
> module** — not an \`.mdx\` doc. It is rendered as **Markdown/MDX** by
> \`components/markdown.tsx\`, so:
> - Inline \`<code>x</code>\`, \`[links](/href)\`, \`**bold**\`, and the custom
>   \`<Hl>…</Hl>\` (primary-colored highlight) are allowed; block elements are not
>   (these slots render inline).
> - **Preserve the leading indentation shown on each line** — \`dedent\` strips the
>   common indentation at runtime, so keep it uniform across every line.
> - Replace **only** the text between the backticks; never emit \`dd\`, the
>   backticks, or a \`${"$"}{...}\` interpolation.
> - Single newlines render as spaces (Markdown), so line wrapping is cosmetic —
>   match the surrounding width.

## Original source (the splice target — rewrite exactly these bytes)

\`\`\`mdx
${original}
\`\`\`

${briefTail(req, file, "terse")}`;
}

/** Build the brief markdown for a request whose doc is at `file`. */
export function buildBrief(req: RewriteRequest, file: string): string {
  if (req.contentKey) return buildContentBrief(req, file);
  const tier: Tier = req.target.tier ?? "terse";
  const { content } = matter(fs.readFileSync(file, "utf8"));
  const original = content.slice(req.target.srcStart, req.target.srcEnd);

  const cShip =
    req.structure === "C"
      ? "\n> ⚠ **Build-C page (two bodies).** This page has a terse-native and a " +
        "detailed-native body. A rewrite to one may require a matching edit to the " +
        "other — keep them consistent.\n"
      : "";

  const spanNote =
    req.target.kind === "span" && req.target.selectedText
      ? `\n## Selected span (narrow the splice to exactly this within the block)\n\n` +
        "```\n" +
        req.target.selectedText +
        "\n```\n"
      : "";

  const nCandidates = req.mode === "candidates" ? "3 candidates" : "1 candidate";

  return `# Rewrite brief — ${req.id}

- **Doc:** /${req.slug.join("/")} — \`${file}\`
- **Purpose:** ${req.purpose ?? "orientation"}   **Structure:** ${req.structure ?? "B"}
- **Target:** ${req.target.kind} · tier \`${tier}\` · content offsets [${req.target.srcStart}, ${req.target.srcEnd}]
- **Mode:** ${req.mode} → produce ${nCandidates}
- **Instruction:** ${JSON.stringify(req.instruction)}
${cShip}
## Original source (the splice target — rewrite exactly these bytes)

\`\`\`mdx
${original}
\`\`\`
${spanNote}
## Grounding — verify before you write

Every claim must match ground truth (the code + git history + runtime), not just the
prose around it. A rewrite that makes a limit disappear or overstates a guarantee is a
regression, however smooth it reads.

- Backing doc: \`${file}\`
- Code truth: this repo — \`CLAUDE.md\`, \`crates/\`, \`examples/*.forge\`. Never assert a
  behavior, limit, or number the code contradicts.

## Style — FOLLOW THIS (spine + \`${registerFile(tier)}\`)

${composeStyle(tier)}
## Output

Write \`.rewrite-queue/proposals/${req.id}.json\` — a RewriteProposal:

\`\`\`jsonc
{
  "id": "${req.id}",
  "srcFile": ${JSON.stringify(file)},
  "srcStart": ${req.target.srcStart},   // authoritative; for a span, NARROW to the selection
  "srcEnd": ${req.target.srcEnd},
  "original": <the current source at [srcStart, srcEnd]>,
  "candidates": [ /* ${nCandidates}: { "text": <raw MDX>, "note"?: <one-line diff> } */ ],
  "mode": ${JSON.stringify(req.mode)}
}
\`\`\`
`;
}

/** Compose and write the brief to `.rewrite-queue/briefs/<id>.md`. Returns its path. */
export function writeBrief(req: RewriteRequest, file: string): string {
  fs.mkdirSync(BRIEFS_DIR, { recursive: true });
  const p = briefPath(req.id);
  fs.writeFileSync(p, buildBrief(req, file));
  return p;
}
