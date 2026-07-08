"use client";

/**
 * The record editor — a right-side drawer of type-aware field controls. Update
 * is whole-record replace (ForgeDB's superseding-version append), never a
 * field-level partial update; the footer says so.
 */

import { useAtom, useSetAtom } from "jotai";
import { toast } from "sonner";
import {
  browseModelAtom,
  closeEditorAtom,
  editorAtom,
} from "@/lib/inspector/atoms";
import { SCHEMA } from "@/lib/inspector/mock";
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

  const fields = SCHEMA[editor.model] ?? [];
  const creating = editor.mode === "create";

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
            Update replaces the whole record
          </span>
          <span className="ml-auto" />
          <Button variant="ghost" size="sm" onClick={() => closeEditor()}>
            Cancel
          </Button>
          <Button size="sm" onClick={save}>
            {creating ? "Insert record" : "Save (replace)"}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
