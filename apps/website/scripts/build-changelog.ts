/**
 * Build the static changelog data (public/changelog.json) that the /changelog
 * page renders, from the repo-root CHANGELOG.md.
 *
 * CHANGELOG.md is the single source of truth — git-cliff generates it from
 * conventional commits at release time (`make changelog`), cargo-dist reads it
 * for the GitHub Release body, and this step parses it for the website. We do
 * NOT keep a second committed copy in the site; instead this prebuild step reads
 * the root file and emits a gitignored JSON snapshot, mirroring the
 * search-index.json precedent.
 *
 * Markdown → HTML happens HERE (marked), deliberately NOT via the site's MDX
 * pipeline: the changelog body is machine-generated from commit subjects, so a
 * stray `<T>` or `{x}` would crash an MDX compile at the worst possible moment
 * (the release-time redeploy). Plain-markdown rendering never throws on those.
 *
 * Graceful degrade: a missing/unreadable CHANGELOG.md writes an empty snapshot
 * and warns rather than failing the build (e.g. a partial checkout).
 */
import fs from "node:fs";
import path from "node:path";
import { marked } from "marked";
import type { ChangelogRelease } from "../lib/changelog";

// The website builds with cwd = apps/website; the changelog lives at the repo root.
const CHANGELOG_PATH = path.join(process.cwd(), "..", "..", "CHANGELOG.md");
const OUT_PATH = path.join(process.cwd(), "public", "changelog.json");

/** A parsed release before its body is rendered to HTML / counted. */
type ParsedRelease = Omit<ChangelogRelease, "html" | "count"> & { body: string };

/** Split CHANGELOG.md into releases on its `## [version] - date` headings. */
export function parseChangelog(raw: string): ParsedRelease[] {
  const releases: ParsedRelease[] = [];
  let current: ParsedRelease | null = null;
  let bodyLines: string[] = [];

  const flush = () => {
    const c = current;
    if (c) {
      c.body = bodyLines.join("\n").trim();
      releases.push(c);
    }
  };

  for (const line of raw.split("\n")) {
    // "## [0.2.0] - 2026-07-28" or "## [Unreleased]"
    const m = /^##\s+\[([^\]]+)\](?:\s*-\s*(.+?))?\s*$/.exec(line);
    if (m) {
      flush();
      const version = (m[1] ?? "").trim();
      const unreleased = /^unreleased$/i.test(version);
      current = { version, unreleased, date: unreleased ? null : (m[2]?.trim() ?? null), body: "" };
      bodyLines = [];
    } else if (current) {
      bodyLines.push(line);
    }
    // Lines before the first `##` (the header preamble) are intentionally dropped.
  }
  flush();
  return releases;
}

function build(): void {
  let raw: string;
  try {
    raw = fs.readFileSync(CHANGELOG_PATH, "utf8");
  } catch {
    fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
    fs.writeFileSync(OUT_PATH, JSON.stringify({ releases: [] }));
    console.warn(`⚠ CHANGELOG.md not found at ${CHANGELOG_PATH} — wrote empty changelog.json`);
    return;
  }

  marked.setOptions({ gfm: true });
  const releases: ChangelogRelease[] = parseChangelog(raw).map((r) => {
    const html = marked.parse(r.body) as string;
    return {
      version: r.version,
      unreleased: r.unreleased,
      date: r.date,
      html,
      count: (html.match(/<li>/g) ?? []).length,
    };
  });

  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(OUT_PATH, JSON.stringify({ releases }));
  console.log(`✓ changelog.json — ${releases.length} release section(s)`);
}

build();
