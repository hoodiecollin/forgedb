import fs from "node:fs";
import path from "node:path";

/** One release section of the changelog, prebuilt into public/changelog.json. */
export interface ChangelogRelease {
  /** "0.2.0" for a tagged release, "Unreleased" for the pending section. */
  version: string;
  /** true for the Unreleased section (no tag / no date yet). */
  unreleased: boolean;
  /** "YYYY-MM-DD" for a tagged release, null for Unreleased. */
  date: string | null;
  /** The release body (### groups + bullet lists) rendered to HTML. */
  html: string;
  /** Number of individual changes (bullet items) in the release. */
  count: number;
}

// Written by scripts/build-changelog.ts in the `prebuild` step (gitignored).
const DATA_PATH = path.join(process.cwd(), "public", "changelog.json");

/**
 * Read the prebuilt changelog snapshot. Returns an empty list if the file is
 * absent (e.g. the prebuild step hasn't run), so the page degrades gracefully
 * rather than crashing the build.
 */
export function getReleases(): ChangelogRelease[] {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_PATH, "utf8")) as { releases?: ChangelogRelease[] };
    return parsed.releases ?? [];
  } catch {
    return [];
  }
}

/** URL-fragment id for a release, e.g. "0.2.0" → "v0-2-0", "Unreleased" → "unreleased". */
export function releaseAnchor(r: Pick<ChangelogRelease, "version" | "unreleased">): string {
  return r.unreleased ? "unreleased" : `v${r.version.replace(/\./g, "-")}`;
}
