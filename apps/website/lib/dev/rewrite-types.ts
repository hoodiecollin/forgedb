/**
 * Shared protocol types for the in-browser prose-rewrite dev tool. Pure types,
 * no runtime — safe to `import type` from client (overlay) and server (route,
 * queue) alike. See `rewrite-queue.ts` for the fs-backed helpers.
 */

/**
 * How a rewrite target was designated in the browser.
 * - `section`/`block`/`span` — offset-based, for `.mdx` docs (remark-source-map).
 * - `content` — key-based, for a typed content module slot (content-target.ts).
 */
export type TargetKind = "section" | "block" | "span" | "content";

/**
 * Which register the target block is written in — picks the style register file
 * (see rewrite-style.ts). The `<DiveDeeper>`/`<ImplementationDetails>` vocabulary
 * exists (data-tier on the rendered block), but the overlay doesn't yet read it,
 * so requests still default to `terse` until that detection is wired.
 */
export type Tier = "terse" | "deeper" | "technical";

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
  /** Register of this block; defaults to `terse` until the overlay reads data-tier. */
  tier?: Tier;
}

/** A request the overlay POSTs to the route; persisted to requests.jsonl. */
export interface RewriteRequest {
  id: string;
  ts: number;
  status: "pending" | "proposed" | "accepted" | "rejected";
  /** URL slug segments of the doc, e.g. ["schema", "overview"]. Empty for content targets. */
  slug: string[];
  /** For content-key targets: the content module id (e.g. "landing") and its slot key. */
  contentModule?: string;
  contentKey?: string;
  target: RewriteTarget;
  /** The user's free-text instruction ("tighten this", "add an example"). */
  instruction: string;
  mode: FeedbackMode;
  /** Content fingerprint of the .mdx body at render time (staleness guard). */
  docHash?: string;
  /** Snapshot of the page's purpose from frontmatter, for register strictness. */
  purpose?: "orientation" | "reference" | "marketing";
  /** Page structure: "C" = two-body (keep terse + detailed in sync); else "B". */
  structure?: "B" | "C";
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
