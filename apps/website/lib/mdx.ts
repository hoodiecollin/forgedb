import fs from "node:fs";
import path from "node:path";
import matter from "gray-matter";

/** All docs MDX lives under content/docs/**. `index.mdx` is the /docs/ root. */
export const DOCS_DIR = path.join(process.cwd(), "content", "docs");

export interface DocFrontmatter {
  title: string;
  description?: string;
}

export interface DocFile {
  /** URL slug segments, e.g. ["schema", "overview"] ("" for the index page). */
  slug: string[];
  /** Canonical href with a trailing slash, e.g. "/docs/schema/overview/". */
  href: string;
  frontmatter: DocFrontmatter;
  content: string;
}

/** Recursively collect every `.mdx` file under content/docs as slug arrays. */
export function getAllDocSlugs(): string[][] {
  const out: string[][] = [];
  const walk = (dir: string, prefix: string[]) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full, [...prefix, entry.name]);
      } else if (entry.isFile() && entry.name.endsWith(".mdx")) {
        const base = entry.name.replace(/\.mdx$/, "");
        out.push(base === "index" ? prefix : [...prefix, base]);
      }
    }
  };
  walk(DOCS_DIR, []);
  return out;
}

function fileForSlug(slug: string[]): string | null {
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

/** Absolute path to the `.mdx` backing a slug, or null. Used by the dev rewrite tool. */
export function resolveDocFile(slug: string[]): string | null {
  return fileForSlug(slug);
}

export interface DocMeta {
  purpose: "orientation" | "reference" | "marketing";
  /** "C" = two-body page (terse + detailed native bodies); else "B". */
  structure: "B" | "C";
}

/** Read the style-relevant frontmatter (purpose, structure) for a slug. Dev rewrite tool. */
export function docMetaForSlug(slug: string[]): DocMeta | null {
  const file = fileForSlug(slug);
  if (!file) return null;
  const { data } = matter(fs.readFileSync(file, "utf8"));
  return {
    purpose: (data.purpose as DocMeta["purpose"]) ?? "orientation",
    structure: data.structure === "C" ? "C" : "B",
  };
}

/** Load a single doc by slug, or null if no matching file exists. */
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

/** Load every doc (used by the search-index builder). */
export function getAllDocs(): DocFile[] {
  return getAllDocSlugs()
    .map((slug) => getDocBySlug(slug))
    .filter((d): d is DocFile => d !== null);
}
