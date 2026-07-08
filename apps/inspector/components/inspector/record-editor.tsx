"use client";

/**
 * The record editor — a right-side drawer of type-aware field controls. Update
 * is whole-record replace (ForgeDB's superseding-version append), never a
 * field-level partial update; the footer says so.
 */

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { toast } from "sonner";
import {
  browseModelAtom,
  closeEditorAtom,
  connectedAtom,
  editorAtom,
  projectSourceAtom,
  schemaAtom,
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

  const fields = schema[editor.model] ?? [];
  const creating = editor.mode === "create";
  // Live against a real API: the generated REST surface is insert-only — there is
  // NO update or delete endpoint (those mutations exist in the DB layer, unexposed).
  // So editing an existing row is read-only; typed insert submission is the next
  // slice (the field controls aren't wired to collect values yet).
  const live = isTauri() && source === "project" && connected;
  const readOnly = live && !creating;

  const followRelation = (target: string, label: string) => {
    closeEditor();
    browse({ model: target, pivot: `${label} of this ${editor.model}` });
  };

  const save = () => {
    closeEditor();
    toast.success(creating ? "Record inserted" : "Record replaced");
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
            {readOnly
              ? "read-only — the generated API is insert-only (no update/delete)"
              : live && creating
                ? "typed insert submission is not wired yet — preview only"
                : "Update replaces the whole record"}
          </span>
          <span className="ml-auto" />
          <Button variant="ghost" size="sm" onClick={() => closeEditor()}>
            {readOnly ? "Close" : "Cancel"}
          </Button>
          {!readOnly ? (
            <Button size="sm" onClick={save} disabled={live}>
              {creating ? "Insert record" : "Save (replace)"}
            </Button>
          ) : null}
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
