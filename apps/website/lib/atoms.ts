import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

/** Whether the ⌘K search palette is open. Shared by the trigger + the dialog. */
export const searchOpenAtom = atom(false);

/**
 * The reader's global verbosity preference — one sticky choice, not per-page.
 *
 * - `terse` (default): show only Tier 1 (the page body). Deeper/implementation
 *   blocks stay collapsed.
 * - `detailed`: expand every deeper/implementation block by default (an
 *   expand-all). On a Build-C page it selects the detailed-native body.
 *
 * Flipping this acts as expand-all / collapse-all: each disclosure re-syncs to
 * it, clearing any local override (see `TierDisclosure`). Persisted so a reader
 * who wants depth keeps it across pages and sessions.
 */
export type DetailLevel = "terse" | "detailed";
export const detailAtom = atomWithStorage<DetailLevel>(
  "forgedb-doc-detail",
  "terse",
);
