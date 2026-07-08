/**
 * Inspector UI state (jotai). Split into small atoms so screens subscribe only
 * to what they render. Editor field-overrides live in nested records keyed by
 * field name (null-state, tri-state bool, struct open/closed).
 */

import { atom } from "jotai";
import { DB_NAME, DEFAULT_PREDICATES, MODELS, REL, SCHEMA } from "./mock";
import type { ProjectStructure } from "./data-source";
import { loadStartupProject, openProject } from "./data-source";
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
/**
 * Base URL of the running generated API for the Live lens (#13). The generated
 * `serve` binds `0.0.0.0:3000` by default; the client issues requests over the
 * Tauri HTTP/WebSocket plugins (CORS-proof).
 */
export const apiBaseAtom = atom("http://localhost:3000");

// ---- project structure (Structure lens, #12) --------------------------------
// The single source screens read schema/model/relation data from. Its default is
// the mock database, so a browser (web dev / static preview) needs no backend.
// Inside Tauri, `openProjectAtom` replaces it with a parsed real `.forge`.
const MOCK_STRUCTURE: ProjectStructure = {
  dbName: DB_NAME,
  models: MODELS,
  rel: REL,
  schema: SCHEMA,
  source: "mock",
  hasStats: false,
};
export const structureAtom = atom<ProjectStructure>(MOCK_STRUCTURE);
export const modelsAtom = atom((get) => get(structureAtom).models);
export const relAtom = atom((get) => get(structureAtom).rel);
export const schemaAtom = atom((get) => get(structureAtom).schema);
export const dbNameAtom = atom((get) => get(structureAtom).dbName);
export const projectSourceAtom = atom((get) => get(structureAtom).source);

export const projectErrorAtom = atom<string | null>(null);
export const projectLoadingAtom = atom(false);

/** Open-project flow (Tauri only): pick a `.forge`, load it into `structureAtom`. */
export const openProjectAtom = atom(null, async (_get, set) => {
  set(projectLoadingAtom, true);
  set(projectErrorAtom, null);
  try {
    const loaded = await openProject();
    if (loaded) {
      const first = loaded.models[0]?.key ?? "";
      set(structureAtom, loaded);
      set(selModelAtom, first);
      set(studioModelAtom, first);
      set(selectionAtom, {});
      set(pivotAtom, null);
      set(predicatesAtom, []); // mock predicates don't apply to a real schema
    }
  } catch (e) {
    set(projectErrorAtom, e instanceof Error ? e.message : String(e));
  } finally {
    set(projectLoadingAtom, false);
  }
});

/**
 * Bootstrap on launch (Tauri only): if a startup project is configured
 * (`FORGEDB_INSPECTOR_PROJECT`), load it; otherwise stay on the mock sample.
 * Runs once from the shell mount.
 */
export const bootstrapProjectAtom = atom(null, async (_get, set) => {
  try {
    const loaded = await loadStartupProject();
    if (loaded) {
      const first = loaded.models[0]?.key ?? "";
      set(structureAtom, loaded);
      set(selModelAtom, first);
      set(studioModelAtom, first);
      set(predicatesAtom, []);
    }
  } catch (e) {
    set(projectErrorAtom, e instanceof Error ? e.message : String(e));
  }
});

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
