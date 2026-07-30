/**
 * Build the static roadmap snapshot (public/roadmap.json) that the /roadmap page
 * renders, from live GitHub issues + milestones + releases.
 *
 * Follows the search-index / changelog precedent: a `prebuild` step writes a
 * gitignored JSON snapshot rather than committing one. The page consumes the
 * snapshot statically (no runtime function) — the caveat banner + link to the
 * live GitHub Project cover the gap to real-time.
 *
 * Auth: uses `gh` — the developer's login locally, and the workflow's
 * `GH_TOKEN` (default GITHUB_TOKEN) in CI. Read-only, public-issue reads.
 *
 * Graceful degrade: if `gh` is missing or any call fails (offline clone, no
 * auth), write an `ok:false` snapshot and warn — the page renders its caveat
 * state rather than failing the site build.
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { buildRoadmap } from "../lib/roadmap-transform";
import type { RawIssue, RawMilestone, RawRelease } from "../lib/roadmap-transform";

const REPO = "hoodiecollin/forgedb";
const OUT_PATH = path.join(process.cwd(), "public", "roadmap.json");

/** Run `gh api <endpoint>` (paginated) and parse the JSON array it prints. */
function ghApi<T>(endpoint: string): T[] {
  const out = execFileSync("gh", ["api", "--paginate", endpoint], {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  // With --paginate, gh concatenates each page's JSON array; `--slurp` would wrap
  // them but isn't on every gh version, so stitch `][` seams into one array.
  return JSON.parse(out.replace(/\]\s*\[/g, ",")) as T[];
}

/** UTC build date "YYYY-MM-DD" for the "snapshot as of" caveat. */
function buildDate(): string {
  return new Date().toISOString().slice(0, 10);
}

function writeDegraded(reason: string): void {
  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(
    OUT_PATH,
    JSON.stringify({
      ok: false,
      generatedAt: buildDate(),
      latestRelease: null,
      pendingRelease: null,
      buckets: { next: [], labs: [], ideas: [] },
      shipped: [],
    }),
  );
  console.warn(`⚠ roadmap: ${reason} — wrote degraded roadmap.json (page shows caveat only)`);
}

function build(): void {
  let issues: RawIssue[];
  let milestones: RawMilestone[];
  let releases: RawRelease[];
  try {
    issues = ghApi<RawIssue>(`repos/${REPO}/issues?state=all&per_page=100`);
    milestones = ghApi<RawMilestone>(`repos/${REPO}/milestones?state=all&per_page=100`);
    releases = ghApi<RawRelease>(`repos/${REPO}/releases?per_page=100`);
  } catch (err) {
    writeDegraded(err instanceof Error ? err.message.split("\n")[0]! : "gh api failed");
    return;
  }

  const data = buildRoadmap(issues, milestones, releases, buildDate());
  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(OUT_PATH, JSON.stringify(data));
  const { next, labs, ideas } = data.buckets;
  console.log(
    `✓ roadmap.json — latest ${data.latestRelease?.tag ?? "none"}, ` +
      `${data.pendingRelease?.done.length ?? 0} pending, ` +
      `${next.length} next / ${labs.length} labs / ${ideas.length} ideas, ` +
      `${data.shipped.length} shipped milestone(s)`,
  );
}

build();
