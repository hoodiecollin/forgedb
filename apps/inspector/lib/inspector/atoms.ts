/**
 * Inspector UI state (jotai). Split into small atoms so screens subscribe only
 * to what they render. Editor field-overrides live in nested records keyed by
 * field name (null-state, tri-state bool, struct open/closed).
 */

import { atom } from "jotai";
import { DEFAULT_PREDICATES } from "./mock";
import type {
  ConsoleTab,
  Lens,
  LiveDeltaKind,
  Predicate,
  Screen,
} from "./types";

// ---- navigation / connection ----
export const screenAtom = atom<Screen>("atlas");
export const connectedAtom = atom(true);

// ---- atlas ----
export const lensAtom = atom<Lens>("live");
/** currently-selected model on the Atlas map */
export const selModelAtom = atom("User");

// ---- studio ----
export const studioModelAtom = atom("User");
/** pivot breadcrumb (set when following a relation into another model) */
export const pivotAtom = atom<string | null>(null);
/** selected row ids in the studio grid */
export const selectionAtom = atom<Record<string, true>>({});
export const liveTailAtom = atom(false);

// ---- filter composer (shared studio + console) ----
export const predicatesAtom = atom<Predicate[]>(DEFAULT_PREDICATES);

// ---- console ----
export const consoleTabAtom = atom<ConsoleTab>("q1");
export const snapPosAtom = atom(68);

// ---- record editor ----
export type EditorMode = "edit" | "create";
export interface EditorState {
  open: boolean;
  model: string;
  rowId: string | null;
  mode: EditorMode;
}
export const editorAtom = atom<EditorState>({
  open: false,
  model: "User",
  rowId: null,
  mode: "edit",
});
/** per-field editor overrides, reset each time the editor opens */
export const editNullsAtom = atom<Record<string, boolean>>({});
export const editBoolsAtom = atom<Record<string, string>>({});
export const editStructsAtom = atom<Record<string, boolean>>({});

// ---- live stream (mock ticker) ----
export interface StreamEvent {
  kind: LiveDeltaKind;
  id: string;
  who: string;
  text: string;
  ts: string;
}
export const streamAtom = atom<StreamEvent[]>([]);

// ---- derived helpers ----
export const isConnectedScreenLiveAtom = atom((get) => {
  const screen = get(screenAtom);
  const connected = get(connectedAtom);
  if (!connected) return false;
  if (screen === "dashboards") return true;
  if (screen === "console" && get(consoleTabAtom) === "live") return true;
  if (screen === "studio" && get(liveTailAtom)) return true;
  return false;
});

/** open the editor fresh (clearing all per-field overrides) */
export const openEditorAtom = atom(
  null,
  (
    _get,
    set,
    payload: { model: string; rowId: string | null; mode: EditorMode },
  ) => {
    set(editorAtom, { open: true, ...payload });
    set(editNullsAtom, {});
    set(editBoolsAtom, {});
    set(editStructsAtom, {});
  },
);

export const closeEditorAtom = atom(null, (get, set) => {
  set(editorAtom, { ...get(editorAtom), open: false });
});

/** navigate to a model's grid, optionally with a pivot breadcrumb */
export const browseModelAtom = atom(
  null,
  (_get, set, payload: { model: string; pivot?: string | null }) => {
    set(screenAtom, "studio");
    set(studioModelAtom, payload.model);
    set(selModelAtom, payload.model);
    set(selectionAtom, {});
    set(pivotAtom, payload.pivot ?? null);
  },
);
