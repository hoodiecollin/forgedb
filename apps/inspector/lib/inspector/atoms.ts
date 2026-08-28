import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
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
export const screenAtom = atom<Screen>("atlas");
export const connectedAtom = atom(true);

export const apiBaseAtom = atomWithStorage(
  "forgedb.inspector.apiBase",
  "http://localhost:3000",
);
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
      set(predicatesAtom, []);
    }
  } catch (e) {
    set(projectErrorAtom, e instanceof Error ? e.message : String(e));
  } finally {
    set(projectLoadingAtom, false);
  }
});
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
export const lensAtom = atom<Lens>("live");
export const selModelAtom = atom("User");
export type SnapshotToken = Record<string, number>;
export const snapshotTokenAtom = atom<SnapshotToken | null>(null);
export interface PinnedSnapshot {
  name: string;
  token: SnapshotToken;
}
export const pinnedSnapshotsAtom = atom<PinnedSnapshot[]>([]);
export const pinSnapshotAtom = atom(
  null,
  async (get, set, name: string): Promise<void> => {
    const { getSnapshotToken } = await import("./live");
    const token = await getSnapshotToken(get(apiBaseAtom));
    set(pinnedSnapshotsAtom, [
      ...get(pinnedSnapshotsAtom).filter((p) => p.name !== name),
      { name, token },
    ]);
    set(snapshotTokenAtom, token);
  },
);
export const studioModelAtom = atom("User");
export const pivotAtom = atom<string | null>(null);
export const selectionAtom = atom<Record<string, true>>({});
export const liveTailAtom = atom(false);
export const predicatesAtom = atom<Predicate[]>(DEFAULT_PREDICATES);
export const consoleTabAtom = atom<ConsoleTab>("q1");
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
export const editNullsAtom = atom<Record<string, boolean>>({});
export const editBoolsAtom = atom<Record<string, string>>({});
export const editStructsAtom = atom<Record<string, boolean>>({});
export const editValuesAtom = atom<Record<string, string>>({});
export const editBaseRowAtom = atom<Record<string, unknown>>({});
export const editSubmittingAtom = atom(false);
export interface StreamEvent {
  kind: LiveDeltaKind;
  id: string;
  who: string;
  text: string;
  ts: string;
}
export const streamAtom = atom<StreamEvent[]>([]);
export const isConnectedScreenLiveAtom = atom((get) => {
  const screen = get(screenAtom);
  const connected = get(connectedAtom);
  if (!connected) return false;
  if (screen === "dashboards") return true;
  if (screen === "console" && get(consoleTabAtom) === "live") return true;
  if (screen === "studio" && get(liveTailAtom)) return true;
  return false;
});
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
    set(editValuesAtom, {});
    set(editBaseRowAtom, {});
  },
);

export const submitEditorAtom = atom(null, async (get, set): Promise<string> => {
  const { buildRecordBody, createRow, updateRow } = await import("./live");
  const editor = get(editorAtom);
  const base = get(apiBaseAtom);
  const fields = get(schemaAtom)[editor.model] ?? [];
  const body = buildRecordBody(
    fields.map((f) => ({ name: f.name, control: f.control, mods: f.mods })),
    get(editBaseRowAtom),
    get(editValuesAtom),
    get(editNullsAtom),
    get(editBoolsAtom),
  );
  set(editSubmittingAtom, true);
  try {
    if (editor.mode === "create") {
      return await createRow(base, editor.model, body);
    }
    await updateRow(base, editor.model, editor.rowId ?? "", body);
    return editor.rowId ?? "";
  } finally {
    set(editSubmittingAtom, false);
  }
});
export const deleteEditorRowAtom = atom(null, async (get, set): Promise<void> => {
  const { deleteRow } = await import("./live");
  const editor = get(editorAtom);
  if (!editor.rowId) return;
  set(editSubmittingAtom, true);
  try {
    await deleteRow(get(apiBaseAtom), editor.model, editor.rowId);
  } finally {
    set(editSubmittingAtom, false);
  }
});
export const closeEditorAtom = atom(null, (get, set) => {
  set(editorAtom, { ...get(editorAtom), open: false });
});
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
