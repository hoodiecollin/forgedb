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

/**
 * The reader's global language-ecosystem preference — one sticky choice that
 * drives every `<Eco>` block across the docs. Switching it swaps the suggested
 * install command and the runtime/SDK usage examples to the selected language;
 * the `.forge` schema and generated-code samples never change (they're
 * language-agnostic).
 *
 * `node` covers Node.js and Bun (they share the generated TypeScript SDK).
 * Persisted two ways: this `atomWithStorage` (localStorage, sticky across pages
 * and sessions) and a `?eco=` URL query param the toggle writes for shareable
 * deep links — a `?eco=` present on load overrides the stored value (see
 * `EcosystemToggle`). Default `node` so SSR and the first client render agree.
 */
export type Ecosystem = "node" | "python" | "rust" | "go";
export const ECOSYSTEMS: Ecosystem[] = ["node", "python", "rust", "go"];
export const DEFAULT_ECOSYSTEM: Ecosystem = "node";
export function isEcosystem(v: string): v is Ecosystem {
  return (ECOSYSTEMS as string[]).includes(v);
}
export const ecosystemAtom = atomWithStorage<Ecosystem>(
  "forgedb-ecosystem",
  DEFAULT_ECOSYSTEM,
);
