/**
 * Client-only helpers that turn a DOM interaction (click / text selection) into
 * one or more candidate rewrite targets, using the `data-src-*` offsets stamped
 * by `remark-source-map`. No fs, no server imports — safe in the overlay bundle.
 */
import type { TargetKind } from "./rewrite-types";

export interface DetectedTarget {
  kind: TargetKind;
  /** Offset-based (docs) targets carry a source range; content targets don't. */
  srcStart?: number;
  srcEnd?: number;
  /** Content-based (marketing) targets carry a content-module slot key instead. */
  contentKey?: string;
  /** For spans: the exact selected text (generator narrows within the block). */
  selectedText: string;
  /** Rendered text of the target, shown to the user. */
  renderedText: string;
  /** Viewport rect to anchor the instruction popover against. */
  rect: DOMRect;
}

/** Nearest ancestor-or-self carrying a stamped source range. */
export function stampedBlock(node: Node | null): HTMLElement | null {
  let el = node instanceof HTMLElement ? node : node?.parentElement ?? null;
  while (el && el.dataset.srcStart === undefined) el = el.parentElement;
  return el && el.dataset.srcStart !== undefined ? el : null;
}

/** Nearest ancestor-or-self carrying a `data-content-key` (content-module slot). */
export function contentBlock(node: Node | null): HTMLElement | null {
  let el = node instanceof HTMLElement ? node : node?.parentElement ?? null;
  while (el && el.dataset.contentKey === undefined) el = el.parentElement;
  return el && el.dataset.contentKey !== undefined ? el : null;
}

/**
 * Content-key target(s) for a click: the nearest element with `data-content-key`,
 * as a single whole-slot target. (Sub-span narrowing within a slot isn't wired —
 * a slot is one string in the content module.)
 */
export function detectContentTargets(clickTarget: Node | null): DetectedTarget[] {
  const el = contentBlock(clickTarget);
  if (!el) return [];
  return [
    {
      kind: "content",
      contentKey: el.dataset.contentKey,
      selectedText: "",
      renderedText: (el.textContent ?? "").trim(),
      rect: el.getBoundingClientRect(),
    },
  ];
}

const startOf = (el: HTMLElement) => Number(el.dataset.srcStart);
const endOf = (el: HTMLElement) => Number(el.dataset.srcEnd);

/** Every stamped heading, in document order, with its depth (h1..h6 → 1..6). */
function headings(): { el: HTMLElement; start: number; depth: number }[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>('[data-src-type="heading"]'),
  )
    .map((el) => ({ el, start: startOf(el), depth: Number(el.tagName[1]) || 6 }))
    .sort((a, b) => a.start - b.start);
}

/** Largest stamped end offset — effectively the document body length. */
function contentEnd(): number {
  let max = 0;
  for (const el of document.querySelectorAll<HTMLElement>("[data-src-end]")) {
    max = Math.max(max, endOf(el));
  }
  return max;
}

/** Section spanning from `headingEl` to the next heading of equal-or-higher rank. */
export function sectionRange(headingEl: HTMLElement): { srcStart: number; srcEnd: number } {
  const hs = headings();
  const start = startOf(headingEl);
  const self = hs.find((h) => h.el === headingEl);
  const depth = self?.depth ?? 6;
  let srcEnd = contentEnd();
  for (const h of hs) {
    if (h.start > start && h.depth <= depth) {
      srcEnd = h.start;
      break;
    }
  }
  return { srcStart: start, srcEnd };
}

const isHeading = (el: HTMLElement) => el.dataset.srcType === "heading";

/**
 * Candidate targets for the current interaction, primary first. A non-collapsed
 * selection yields [span, enclosing-block]; a click on a heading yields
 * [section, heading-block]; any other click yields [block].
 */
export function detectTargets(clickTarget: Node | null): DetectedTarget[] {
  const sel = window.getSelection();
  const block = stampedBlock(clickTarget);

  // Text selection → span (with the enclosing block as the widen-to alternative).
  if (sel && !sel.isCollapsed && sel.rangeCount > 0) {
    const range = sel.getRangeAt(0);
    const host = stampedBlock(range.commonAncestorContainer);
    if (host) {
      const rect = range.getBoundingClientRect();
      const selectedText = sel.toString();
      return [
        {
          kind: "span",
          srcStart: startOf(host),
          srcEnd: endOf(host),
          selectedText,
          renderedText: selectedText,
          rect,
        },
        {
          kind: "block",
          srcStart: startOf(host),
          srcEnd: endOf(host),
          selectedText: "",
          renderedText: (host.textContent ?? "").trim(),
          rect,
        },
      ];
    }
  }

  if (!block) return [];
  const rect = block.getBoundingClientRect();
  const rendered = (block.textContent ?? "").trim();

  // Heading click → section (primary) or the heading line alone.
  if (isHeading(block)) {
    const section = sectionRange(block);
    return [
      { kind: "section", ...section, selectedText: "", renderedText: rendered, rect },
      {
        kind: "block",
        srcStart: startOf(block),
        srcEnd: endOf(block),
        selectedText: "",
        renderedText: rendered,
        rect,
      },
    ];
  }

  return [
    {
      kind: "block",
      srcStart: startOf(block),
      srcEnd: endOf(block),
      selectedText: "",
      renderedText: rendered,
      rect,
    },
  ];
}
