/**
 * Build the static roadmap snapshot (public/roadmap.json) that the /roadmap page
 * renders, from live GitHub issues + milestones + releases + epic sub-issues.
 *
 * Follows the search-index / changelog precedent: a `prebuild` step writes a
 * gitignored JSON snapshot rather than committing one. The page consumes it
 * statically (no runtime function) — the caveat banner + link to the live
 * GitHub Project cover the gap to real-time.
 *
 * Auth: uses `gh` — the developer's login locally, the workflow's `GH_TOKEN`
 * (default GITHUB_TOKEN) in CI. Read-only, public-issue reads.
 *
 * Graceful degrade: if `gh` is missing or any call fails, write an `ok:false`
 * snapshot and warn — the page renders its caveat state rather than failing.
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { buildRoadmap } from "../lib/roadmap-transform";
import type { RawIssue, RawMilestone, RawRelease, RawEpic } from "../lib/roadmap-transform";

const REPO = "hoodiecollin/forgedb";
const OUT_PATH = path.join(process.cwd(), "public", "roadmap.json");

/** Run `gh api <endpoint>` (paginated) and parse the JSON array it prints. */
function ghApi<T>(endpoint: string): T[] {
  const out = execFileSync("gh", ["api", "--paginate", endpoint], {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  // With --paginate, gh concatenates each page's JSON array; stitch `][` seams.
  return JSON.parse(out.replace(/\]\s*\[/g, ",")) as T[];
}

function buildDate(): string {
  return new Date().toISOString().slice(0, 10);
}

const DEGRADED = {
  ok: false as const,
  generatedAt: buildDate(),
  latestRelease: null,
  nextMilestone: null,
  active: [],
  planned: [],
  labs: [],
  ideas: [],
  shippedEpics: [],
  releases: [],
};

function writeDegraded(reason: string): void {
  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(OUT_PATH, JSON.stringify(DEGRADED));
  console.warn(`⚠ roadmap: ${reason} — wrote degraded roadmap.json (page shows caveat only)`);
}

function build(): void {
  let issues: RawIssue[];
  let milestones: RawMilestone[];
  let releases: RawRelease[];
  let epics: RawEpic[];
  try {
    issues = ghApi<RawIssue>(`repos/${REPO}/issues?state=all&per_page=100`);
    milestones = ghApi<RawMilestone>(`repos/${REPO}/milestones?state=all&per_page=100`);
    releases = ghApi<RawRelease>(`repos/${REPO}/releases?per_page=100`);
    // Each epic's native sub-issues (children). Epics are issues labeled `epic`.
    const epicNumbers = issues
      .filter((i) => !i.pull_request && i.labels.some((l) => l.name.toLowerCase() === "epic"))
      .map((i) => i.number);
    epics = epicNumbers.map((n) => ({
      number: n,
      children: ghApi<RawIssue>(`repos/${REPO}/issues/${n}/sub_issues?per_page=100`),
    }));
  } catch (err) {
    writeDegraded(err instanceof Error ? err.message.split("\n")[0]! : "gh api failed");
    return;
  }

  const data = buildRoadmap(issues, milestones, releases, epics, buildDate());
  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(OUT_PATH, JSON.stringify(data));
  const epicCount = (arr: typeof data.active) => arr.filter((i) => i.kind === "epic").length;
  console.log(
    `✓ roadmap.json — latest ${data.latestRelease?.tag ?? "none"}, next ${data.nextMilestone?.title ?? "none"}; ` +
      `active ${data.active.length} (${epicCount(data.active)} epic) / planned ${data.planned.length} / ` +
      `labs ${data.labs.length} / ideas ${data.ideas.length}; ` +
      `${data.releases.length} release card(s)`,
  );
}

build();
