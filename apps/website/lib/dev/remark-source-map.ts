import type { Root, RootContent } from "mdast";
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
