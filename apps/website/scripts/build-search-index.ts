import fs from "node:fs";
import path from "node:path";
import { getAllDocs, hrefForSlug } from "../lib/mdx";
import { flatDocs, docMeta } from "../lib/docs-nav";
import { extractToc } from "../lib/toc";
import type { SearchDoc } from "../lib/search";
function toPlainText(mdx: string): string {
  return mdx
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`[^`]+`/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[#>*_~|-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}
const docs = getAllDocs();
const index: SearchDoc[] = docs.map((d) => {
  const meta = docMeta(d.href);
  return {
    title: d.frontmatter.title,
    href: d.href,
    group: meta.group ?? "Docs",
    description: d.frontmatter.description ?? "",
    headings: extractToc(d.content).map((h) => h.text),
    excerpt: toPlainText(d.content).slice(0, 240),
  };
});
const order = new Map(flatDocs.map((i, n) => [i.href, n]));
index.sort((a, b) => (order.get(a.href) ?? 999) - (order.get(b.href) ?? 999));
const fileHrefs = new Set(docs.map((d) => d.href));
const navHrefs = new Set(flatDocs.map((i) => i.href));
const missing = flatDocs.filter((i) => !fileHrefs.has(i.href));
const orphans = docs.filter((d) => !navHrefs.has(d.href));
if (missing.length) {
  console.error("\n✗ docs nav references pages with no MDX file:");
  for (const m of missing) console.error(`   ${m.href}  (expected content/docs${m.href.replace(/^\/docs/, "").replace(/\/$/, "") || "/index"}.mdx)`);
}
if (orphans.length) {
  console.warn("\n⚠ MDX files not in the sidebar nav (add to lib/docs-nav.ts):");
  for (const o of orphans) console.warn(`   ${o.href}`);
}
const outDir = path.join(process.cwd(), "public");
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(path.join(outDir, "search-index.json"), JSON.stringify(index));
console.log(`\n✓ search-index.json — ${index.length} pages indexed`);

if (missing.length) {
  console.error(`\n✗ ${missing.length} dead nav link(s) — see above.`);
  process.exit(1);
}

void hrefForSlug;
