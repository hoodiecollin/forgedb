/**
 * Shared protocol types for the in-browser prose-rewrite dev tool. Pure types,
 * no runtime — safe to `import type` from client (overlay) and server (route,
 * queue) alike. See `rewrite-queue.ts` for the fs-backed helpers.
 */

/** How a rewrite target was designated in the browser. */
export type TargetKind = "section" | "block" | "span";

/**
 * How a proposal is surfaced before it touches the file.
 * - `diff`   — one rewrite, shown as a diff to accept/reject (default).
 * - `candidates` — N alternatives to pick from.
 */
export type FeedbackMode = "diff" | "candidates";

/** What the user pointed at, in content-space source offsets (see remark-source-map). */
export interface RewriteTarget {
  kind: TargetKind;
  /** Char offset into the MDX body (post-frontmatter) where the target starts. */
  srcStart: number;
  /** Char offset where the target ends. */
  srcEnd: number;
  /**
   * For `span`: the exact selected text. The block range above is the enclosing
   * block; the generator narrows to the precise substring within it. Empty for
   * section/block.
   */
  selectedText: string;
  /** Rendered text of the target, for context + showing the user what they picked. */
  renderedText: string;
}

/** A request the overlay POSTs to the route; persisted to requests.jsonl. */
export interface RewriteRequest {
  id: string;
  ts: number;
  status: "pending" | "proposed" | "accepted" | "rejected";
  /** URL slug segments of the doc, e.g. ["schema", "overview"]. */
  slug: string[];
  target: RewriteTarget;
  /** The user's free-text instruction ("tighten this", "add an example"). */
  instruction: string;
  mode: FeedbackMode;
  /** Content fingerprint of the .mdx body at render time (staleness guard). */
  docHash?: string;
}

/** One candidate rewrite within a proposal. */
export interface RewriteCandidate {
  /** The replacement source text (raw MDX, spliced verbatim into the file). */
  text: string;
  /** Optional one-line note on what this variant does differently. */
  note?: string;
}

/**
 * The generator's answer, written by Claude to proposals/<id>.json and served
 * to the polling overlay. `srcStart`/`srcEnd` are authoritative for the splice
 * — for a span they are narrowed to the exact selection, not the block.
 */
export interface RewriteProposal {
  id: string;
  srcFile: string;
  srcStart: number;
  srcEnd: number;
  /** The current source at [srcStart, srcEnd] — the "before" side of the diff. */
  original: string;
  candidates: RewriteCandidate[];
  mode: FeedbackMode;
}
