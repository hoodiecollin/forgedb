/**
 * Deterministic content fingerprint for the rewrite tool's staleness guard.
 * Stamped into the page at render time and re-checked server-side at submit and
 * accept: if the .mdx body changed since the browser rendered it, the stamped
 * `data-src-*` offsets are stale and a splice would corrupt the file, so the
 * write is refused. Hashes the MDX *body* (post-frontmatter) — the coordinate
 * space the offsets live in — so frontmatter-only edits don't false-trip it.
 *
 * djb2 + length; not cryptographic, just a cheap change detector.
 */
export function hashContent(s: string): string {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) >>> 0;
  return `${h.toString(36)}-${s.length.toString(36)}`;
}
