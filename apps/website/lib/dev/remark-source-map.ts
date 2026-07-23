/**
 * Dev-only remark plugin: stamps each block-level mdast node with its source
 * range so the in-browser rewrite tool can map a rendered element back to an
 * exact slice of the `.mdx`.
 *
 * Offsets are **content-space** — i.e. character offsets into the MDX body that
 * `next-mdx-remote` was handed (`doc.content`, already stripped of frontmatter
 * by gray-matter). The route handler re-derives the same content string with
 * gray-matter and splices there, so both ends agree on the coordinate system.
 *
 * This must never run in the production export build — it is added to the
 * remark pipeline only when `NODE_ENV === "development"` (see the docs page).
 * It mutates `data.hProperties`, so the attributes ride through mdast→hast into
 * the DOM as plain `data-*` attributes.
 */
import type { Root, RootContent } from "mdast";

/** Block-level node types worth stamping (the units a person selects). */
const STAMPED = new Set([
  "paragraph",
  "heading",
  "list",
  "listItem",
  "blockquote",
  "code",
  "table",
  "thematicBreak",
  "html",
]);

interface HasData {
  type: string;
  position?: { start: { offset?: number }; end: { offset?: number } };
  data?: { hProperties?: Record<string, unknown> };
  children?: RootContent[];
}

function stamp(node: HasData): void {
  const start = node.position?.start?.offset;
  const end = node.position?.end?.offset;
  if (STAMPED.has(node.type) && start != null && end != null) {
    const data = (node.data ??= {});
    const props = (data.hProperties ??= {});
    props["data-src-start"] = String(start);
    props["data-src-end"] = String(end);
    props["data-src-type"] = node.type;
    // Stable-enough id for React keys and re-selection within one render.
    props["data-block-id"] = `${node.type}-${start}`;
  }
  if (node.children) for (const child of node.children) stamp(child as HasData);
}

export function remarkSourceMap() {
  return (tree: Root) => {
    stamp(tree as unknown as HasData);
  };
}

export default remarkSourceMap;
