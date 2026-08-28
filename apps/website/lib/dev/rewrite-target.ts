import type { TargetKind } from "./rewrite-types";
export interface DetectedTarget {
  kind: TargetKind;
  srcStart?: number;
  srcEnd?: number;
  contentKey?: string;
  selectedText: string;
  renderedText: string;
  rect: DOMRect;
}
export function stampedBlock(node: Node | null): HTMLElement | null {
  let el = node instanceof HTMLElement ? node : node?.parentElement ?? null;
  while (el && el.dataset.srcStart === undefined) el = el.parentElement;
  return el && el.dataset.srcStart !== undefined ? el : null;
}
export function contentBlock(node: Node | null): HTMLElement | null {
  let el = node instanceof HTMLElement ? node : node?.parentElement ?? null;
  while (el && el.dataset.contentKey === undefined) el = el.parentElement;
  return el && el.dataset.contentKey !== undefined ? el : null;
}
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
function headings(): { el: HTMLElement; start: number; depth: number }[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>('[data-src-type="heading"]'),
  )
    .map((el) => ({ el, start: startOf(el), depth: Number(el.tagName[1]) || 6 }))
    .sort((a, b) => a.start - b.start);
}
function contentEnd(): number {
  let max = 0;
  for (const el of document.querySelectorAll<HTMLElement>("[data-src-end]")) {
    max = Math.max(max, endOf(el));
  }
  return max;
}
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
export function detectTargets(clickTarget: Node | null): DetectedTarget[] {
  const sel = window.getSelection();
  const block = stampedBlock(clickTarget);

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
