import fs from "node:fs";
import path from "node:path";

/**
 * Forward status of a roadmap item. The roadmap is EPIC-PRIMARY: epics are the
 * top-level unit and may span releases, so status is derived from an item's own
 * state + its children, not from a single milestone. `when` (which release) is a
 * per-item/per-child annotation, not the primary axis.
 */
export type Status = "active" | "planned" | "labs" | "ideas";

/** A sub-issue of an epic, annotated with where it lands. */
export interface ChildIssue {
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  /** Milestone title the child targets, or null. */
  milestone: string | null;
  /** closed AND its milestone has a published release. */
  shipped: boolean;
  /** closed AND milestoned but the release isn't out yet (done, awaiting tag). */
  pending: boolean;
}

/** An epic — a top-level, collapsible initiative that can span releases. */
export interface EpicItem {
  kind: "epic";
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  labels: string[];
  status: Status;
  children: ChildIssue[];
  /** Progress: closed children / total children. */
  done: number;
  total: number;
}

/** A standalone issue — no epic parent (a bug fix, a one-off). */
export interface IssueItem {
  kind: "issue";
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  labels: string[];
  milestone: string | null;
  shipped: boolean;
  pending: boolean;
  status: Status;
}

export type RoadmapItem = EpicItem | IssueItem;

/** A published core milestone, for the release-overview cards. */
export interface ShippedMilestone {
  title: string;
  url: string;
  releaseUrl: string;
  date: string | null;
  closed: number;
}

/** The full roadmap snapshot prebuilt into public/roadmap.json. */
export interface RoadmapData {
  ok: boolean;
  generatedAt: string;
  latestRelease: { tag: string; url: string; date: string | null } | null;
  /** The next release in flight (open core milestone), for header context. */
  nextMilestone: { title: string; url: string; done: number; open: number } | null;
  active: RoadmapItem[];
  planned: RoadmapItem[];
  labs: RoadmapItem[];
  ideas: RoadmapItem[];
  /** Closed epics (grouped in the Shipped section), if any. */
  shippedEpics: EpicItem[];
  /** Compact release cards (newest first) for the Shipped overview. */
  releases: ShippedMilestone[];
}

const DATA_PATH = path.join(process.cwd(), "public", "roadmap.json");

const EMPTY: RoadmapData = {
  ok: false,
  generatedAt: "",
  latestRelease: null,
  nextMilestone: null,
  active: [],
  planned: [],
  labs: [],
  ideas: [],
  shippedEpics: [],
  releases: [],
};

/**
 * Read the prebuilt roadmap snapshot. Returns a degraded-but-valid shape if the
 * file is absent/unreadable (e.g. the prebuild couldn't reach GitHub), so the
 * page renders its caveat rather than crashing the build.
 */
export function getRoadmap(): RoadmapData {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_PATH, "utf8")) as Partial<RoadmapData>;
    return { ...EMPTY, ...parsed };
  } catch {
    return EMPTY;
  }
}

/** The public GitHub Project board, linked from the "for the live view" caveat. */
export const GH_PROJECT_URL = "https://github.com/users/hoodiecollin/projects/3";
