"use client";

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { useEffect } from "react";
import { toast } from "sonner";
import {
  apiBaseAtom,
  browseModelAtom,
  closeEditorAtom,
  connectedAtom,
  deleteEditorRowAtom,
  editBaseRowAtom,
  editSubmittingAtom,
  editValuesAtom,
  editorAtom,
  projectSourceAtom,
  schemaAtom,
  submitEditorAtom,
} from "@/lib/inspector/atoms";
import { isTauri } from "@/lib/inspector/data-source";
import { FieldControl } from "./field-control";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
export function RecordEditor() {
  const [editor] = useAtom(editorAtom);
  const closeEditor = useSetAtom(closeEditorAtom);
  const browse = useSetAtom(browseModelAtom);
  const schema = useAtomValue(schemaAtom);
  const source = useAtomValue(projectSourceAtom);
  const connected = useAtomValue(connectedAtom);
  const apiBase = useAtomValue(apiBaseAtom);
  const submit = useSetAtom(submitEditorAtom);
  const del = useSetAtom(deleteEditorRowAtom);
  const setBaseRow = useSetAtom(editBaseRowAtom);
  const setValues = useSetAtom(editValuesAtom);
  const submitting = useAtomValue(editSubmittingAtom);
  const fields = schema[editor.model] ?? [];
  const creating = editor.mode === "create";
  const live = isTauri() && source === "project" && connected;
  useEffect(() => {
    if (!editor.open || !live || creating || !editor.rowId) return;
    let cancelled = false;
    void (async () => {
      try {
        const { getRow } = await import("@/lib/inspector/live");
        const row = await getRow(apiBase, editor.model, editor.rowId ?? "");
        if (cancelled || !row) return;
        setBaseRow(row);
        const seed: Record<string, string> = {};
        for (const f of fields) {
          const v = row[f.name];
          if (v !== null && v !== undefined && typeof v !== "object") {
            seed[f.name] = String(v);
          }
        }
        setValues(seed);
      } catch (e) {
        toast.error(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor.open, editor.model, editor.rowId, live, creating]);
  const followRelation = (target: string, label: string) => {
    closeEditor();
    browse({ model: target, pivot: `${label} of this ${editor.model}` });
  };
  const onSave = async () => {
    if (!live) {
      closeEditor();
      toast.success(creating ? "Record inserted (preview)" : "Record replaced (preview)");
      return;
    }
    try {
      await submit();
      closeEditor();
      toast.success(creating ? "Record inserted" : "Record replaced");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };
  const onDelete = async () => {
    try {
      await del();
      closeEditor();
      toast.success("Record deleted");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <Sheet open={editor.open} onOpenChange={(o) => !o && closeEditor()}>
      <SheetContent className="flex w-[460px] max-w-[92vw] flex-col gap-0 p-0 sm:max-w-[460px]">
        <SheetHeader className="border-b border-border p-4">
          <SheetTitle className="flex items-center gap-2 text-[15px]">
            {creating ? "New " : "Edit "}
            {editor.model}
          </SheetTitle>
          <div className="font-mono text-[11.5px] text-muted-foreground">
            {creating
              ? "auto fields assigned on insert"
              : `${editor.rowId ?? ""} · whole-record update`}
          </div>
        </SheetHeader>
        <div className="flex flex-1 flex-col gap-4 overflow-auto p-4">
          {fields.map((f) => (
            <FieldControl
              key={f.name}
              field={f}
              onFollowRelation={followRelation}
            />
          ))}
        </div>
        <SheetFooter className="flex-row items-center gap-2.5 border-t border-border p-3.5">
          <span className="text-[11.5px] text-muted-foreground">
            {live
              ? "PUT replaces the whole record (superseding-version append)"
              : "Update replaces the whole record"}
          </span>
          <span className="ml-auto" />
          {live && !creating ? (
            <Button
              variant="ghost"
              size="sm"
              className="text-danger hover:text-danger"
              onClick={onDelete}
              disabled={submitting}
            >
              Delete
            </Button>
          ) : null}
          <Button variant="ghost" size="sm" onClick={() => closeEditor()}>
            Cancel
          </Button>
          <Button size="sm" onClick={onSave} disabled={submitting}>
            {creating ? "Insert record" : "Save (replace)"}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
