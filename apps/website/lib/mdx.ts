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

/**
 * The URL segment that selects a Build-C page's detailed-native body. A page
 * `foo.mdx` (terse, canonical) may have a sibling `foo.detailed.mdx` rendered
 * at `/docs/.../foo/detailed/`. Both are statically generated; the terse body
 * is canonical and search-indexed, the detailed variant is reached via the
 * on-page toggle.
 */
export const DETAILED_SEGMENT = "detailed";

/**
 * Absolute path to the `.detailed.mdx` body backing a *base* (non-detailed) slug.
 * The docs root (`[]`) is served by `index.mdx`, so its detailed sibling is
 * `index.detailed.mdx` at `/docs/detailed/`; every other page is a leaf `X.mdx`
 * paired with `X.detailed.mdx`.
 */
function detailedFileForBase(base: string[]): string {
  return base.length === 0
    ? path.join(DOCS_DIR, "index.detailed.mdx")
    : path.join(DOCS_DIR, ...base) + ".detailed.mdx";
}

/** Recursively collect every `.mdx` file under content/docs as slug arrays. */
export function getAllDocSlugs(): string[][] {
  const out: string[][] = [];
  const walk = (dir: string, prefix: string[]) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full, [...prefix, entry.name]);
      } else if (entry.isFile() && entry.name.endsWith(".detailed.mdx")) {
        // `foo.detailed.mdx` -> [..., "foo", "detailed"]; `index.detailed.mdx`
        // is a directory page's detailed sibling, so it drops the `index`
        // segment (root -> /docs/detailed/), mirroring how `index.mdx` maps.
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

/** Whether a slug names a Build-C detailed variant with a backing file. */
export function isDetailedSlug(slug: string[]): boolean {
  if (slug.length === 0 || slug[slug.length - 1] !== DETAILED_SEGMENT) return false;
  return fs.existsSync(detailedFileForBase(slug.slice(0, -1)));
}

/** Whether a (base, non-detailed) slug has a detailed-native sibling — i.e. a Build-C page. */
export function hasDetailedVariant(slug: string[]): boolean {
  return fs.existsSync(detailedFileForBase(slug));
}

function fileForSlug(slug: string[]): string | null {
  // A detailed variant resolves to its `.detailed.mdx` sibling, when present.
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

/**
 * Load every canonical doc (used by the search-index builder). Detailed-native
 * Build-C variants are excluded — the terse body is the canonical, indexed page.
 */
export function getAllDocs(): DocFile[] {
  return getAllDocSlugs()
    .filter((slug) => !isDetailedSlug(slug))
    .map((slug) => getDocBySlug(slug))
    .filter((d): d is DocFile => d !== null);
}
