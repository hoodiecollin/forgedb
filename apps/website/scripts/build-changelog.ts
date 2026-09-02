import fs from "node:fs";
import path from "node:path";
import { marked } from "marked";
import type { ChangelogRelease } from "../lib/changelog";
const CHANGELOG_PATH = path.join(process.cwd(), "..", "..", "CHANGELOG.md");
const OUT_PATH = path.join(process.cwd(), "public", "changelog.json");
type ParsedRelease = Omit<ChangelogRelease, "html" | "count"> & { body: string };
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
