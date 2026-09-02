import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

export const searchOpenAtom = atom(false);

export type DetailLevel = "terse" | "detailed";
export const detailAtom = atomWithStorage<DetailLevel>(
  "forgedb-doc-detail",
  "terse",
);
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
