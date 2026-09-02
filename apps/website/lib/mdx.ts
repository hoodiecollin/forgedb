import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";

export const DOCS_DIR = path.join(process.cwd(), "content", "docs");

export interface DocFrontmatter {
  title: string;
  description?: string;
}

export interface DocFile {
  slug: string[];
  href: string;
  frontmatter: DocFrontmatter;
  content: string;
}

export const DETAILED_SEGMENT = "detailed";
function detailedFileForBase(base: string[]): string {
  return base.length === 0
    ? path.join(DOCS_DIR, "index.detailed.mdx")
    : path.join(DOCS_DIR, ...base) + ".detailed.mdx";
}
export function getAllDocSlugs(): string[][] {
  const out: string[][] = [];
  const walk = (dir: string, prefix: string[]) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full, [...prefix, entry.name]);
      } else if (entry.isFile() && entry.name.endsWith(".detailed.mdx")) {

        const base = entry.name.replace(/\.detailed\.mdx$/, "");
        out.push(base === "index" ? [...prefix, DETAILED_SEGMENT] : [...prefix, base, DETAILED_SEGMENT]);
      } else if (entry.isFile() && entry.name.endsWith(".mdx")) {
        const base = entry.name.replace(/\.mdx$/, "");
        out.push(base === "index" ? prefix : [...prefix, base]);
      }
    }
  };
  walk(DOCS_DIR, []);
  return out;
}
export function isDetailedSlug(slug: string[]): boolean {
  if (slug.length === 0 || slug[slug.length - 1] !== DETAILED_SEGMENT) return false;
  return fs.existsSync(detailedFileForBase(slug.slice(0, -1)));
}
export function hasDetailedVariant(slug: string[]): boolean {
  return fs.existsSync(detailedFileForBase(slug));
}

function fileForSlug(slug: string[]): string | null {
  if (slug.length >= 1 && slug[slug.length - 1] === DETAILED_SEGMENT) {
    const detailed = detailedFileForBase(slug.slice(0, -1));
    if (fs.existsSync(detailed)) return detailed;
  }
  const candidates =
    slug.length === 0
      ? [path.join(DOCS_DIR, "index.mdx")]
      : [
          path.join(DOCS_DIR, ...slug) + ".mdx",
          path.join(DOCS_DIR, ...slug, "index.mdx"),
        ];
  return candidates.find((p) => fs.existsSync(p)) ?? null;
}
export function hrefForSlug(slug: string[]): string {
  return slug.length === 0 ? "/docs/" : `/docs/${slug.join("/")}/`;
}
export function resolveDocFile(slug: string[]): string | null {
  return fileForSlug(slug);
}
export interface DocMeta {
  purpose: "orientation" | "reference" | "marketing";
  structure: "B" | "C";
}

export function docMetaForSlug(slug: string[]): DocMeta | null {
  const file = fileForSlug(slug);
  if (!file) return null;
  const { data } = matter(fs.readFileSync(file, "utf8"));
  return {
    purpose: (data.purpose as DocMeta["purpose"]) ?? "orientation",
    structure: data.structure === "C" ? "C" : "B",
  };
}

export function getDocBySlug(slug: string[]): DocFile | null {
  const file = fileForSlug(slug);
  if (!file) return null;
  const raw = fs.readFileSync(file, "utf8");
  const { data, content } = matter(raw);
  return {
    slug,
    href: hrefForSlug(slug),
    frontmatter: {
      title: (data.title as string) ?? "Untitled",
      description: data.description as string | undefined,
    },
    content,
  };
}
export function getAllDocs(): DocFile[] {
  return getAllDocSlugs()
    .filter((slug) => !isDetailedSlug(slug))
    .map((slug) => getDocBySlug(slug))
    .filter((d): d is DocFile => d !== null);
}
