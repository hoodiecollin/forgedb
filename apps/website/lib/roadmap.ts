import fs from "node:fs";
import path from "node:path";

/** A single GitHub issue as it appears on the roadmap. */
export interface RoadmapIssue {
  number: number;
  title: string;
  /** github.com issue URL. */
  url: string;
  /** The issue's label names (lowercased), for the small tag chips. */
  labels: string[];
}

/** A published core milestone (has a matching non-prerelease GitHub Release). */
export interface ShippedMilestone {
  /** e.g. "v0.2.1". */
  title: string;
  /** Milestone page URL (its closed-issue list). */
  url: string;
  /** The matching GitHub Release URL. */
  releaseUrl: string;
  /** Release publish date "YYYY-MM-DD", or null. */
  date: string | null;
  /** Count of issues closed under the milestone. */
  closed: number;
}

/** The forward-looking buckets: open issues grouped by their label taxonomy. */
export interface RoadmapBuckets {
  /** Committed, releasable work — open `plan-next` / `epic`. */
  next: RoadmapIssue[];
  /** Experiments / RFCs to measure — open `experiment` / `rfc`. */
  labs: RoadmapIssue[];
  /** Speculative, needs design — open `idea`. */
  ideas: RoadmapIssue[];
}

/** Done-but-not-yet-tagged: closed issues in a core milestone with no release. */
export interface PendingRelease {
  /** e.g. "v0.3.0". */
  milestone: string;
  url: string;
  /** Closed issues awaiting the tag (the interesting, changelog-not-yet set). */
  done: RoadmapIssue[];
  /** How many issues remain open under the milestone. */
  openCount: number;
}

/** The full roadmap snapshot prebuilt into public/roadmap.json. */
export interface RoadmapData {
  /** true when the snapshot was built from live GitHub data. */
  ok: boolean;
  /** Build date "YYYY-MM-DD" (UTC) — shown in the "snapshot as of" caveat. */
  generatedAt: string;
  /** The latest published core release, headlining the waterline. */
  latestRelease: { tag: string; url: string; date: string | null } | null;
  pendingRelease: PendingRelease | null;
  buckets: RoadmapBuckets;
  /** Published core milestones, newest first (compact — detail lives in /changelog). */
  shipped: ShippedMilestone[];
}

// Written by scripts/build-roadmap.ts in the `prebuild` step (gitignored).
const DATA_PATH = path.join(process.cwd(), "public", "roadmap.json");

const EMPTY: RoadmapData = {
  ok: false,
  generatedAt: "",
  latestRelease: null,
  pendingRelease: null,
  buckets: { next: [], labs: [], ideas: [] },
  shipped: [],
};

/**
 * Read the prebuilt roadmap snapshot. Returns a degraded-but-valid shape if the
 * file is absent or unreadable (e.g. the prebuild step couldn't reach GitHub),
 * so the page renders the caveat rather than crashing the build.
 */
export function getRoadmap(): RoadmapData {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_PATH, "utf8")) as Partial<RoadmapData>;
    return { ...EMPTY, ...parsed, buckets: { ...EMPTY.buckets, ...parsed.buckets } };
  } catch {
    return EMPTY;
  }
}

/** The public GitHub Project board, linked from the "for the live view" caveat. */
export const GH_PROJECT_URL = "https://github.com/users/hoodiecollin/projects/3";
