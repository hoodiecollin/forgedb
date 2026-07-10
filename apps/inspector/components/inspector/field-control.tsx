"use client";

/**
 * The type-aware record-editor field. Maps every ForgeDB field type to the
 * right control and surfaces the schema's real semantics — and its real honest
 * limits (see docs/forgedb-inspector-design-review.md): nullable ≠ empty,
 * tri-state bool, u64 precision, whole-record replace, no M2M unlink, linear
 * reverse scans.
 */

import { useAtom } from "jotai";
import { ArrowRight, Copy } from "lucide-react";
import {
  editBoolsAtom,
  editNullsAtom,
  editStructsAtom,
  editValuesAtom,
} from "@/lib/inspector/atoms";
import type { Field, Mod } from "@/lib/inspector/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

const MOD_META: Record<Mod, { label: string; cls: string }> = {
  "+": { label: "auto-generated", cls: "text-ok border-ok/50" },
  "&": { label: "unique", cls: "text-warn border-warn/50" },
  "^": { label: "indexed", cls: "text-info border-info/50" },
  "?": { label: "nullable", cls: "text-muted-foreground border-border" },
};

const SCALAR_NULLABLE = new Set([
  "string",
  "text",
  "int",
  "bigint",
  "float",
  "char",
  "ts",
]);

function ModBadge({ mod }: { mod: Mod }) {
  const m = MOD_META[mod];
  return (
    <span
      title={m.label}
      className={cn(
        "rounded border px-1 font-mono text-[10px] leading-4 font-bold",
        m.cls,
      )}
    >
      {mod}
    </span>
  );
}

export function FieldControl({
  field,
  onFollowRelation,
}: {
  field: Field;
  onFollowRelation: (target: string, label: string) => void;
}) {
  const [nulls, setNulls] = useAtom(editNullsAtom);
  const [bools, setBools] = useAtom(editBoolsAtom);
  const [structs, setStructs] = useAtom(editStructsAtom);
  const [values, setValues] = useAtom(editValuesAtom);

  // Controlled scalar value: the edited override, else the seeded/base value.
  const val = (fallback?: string) => values[field.name] ?? fallback ?? "";
  const onVal = (v: string) => setValues({ ...values, [field.name]: v });

  const nullable = field.mods.includes("?");
  const autoGen = field.mods.includes("+");
  const c = field.control;
  const scalarNullable = nullable && SCALAR_NULLABLE.has(c);
  const isNull = nulls[field.name] === true;
  const boolVal = bools[field.name] ?? field.default ?? "null";
  const structOpen = structs[field.name] !== false;

  const toggleNull = () =>
    setNulls({ ...nulls, [field.name]: !isNull });
  const toggleStruct = () =>
    setStructs({ ...structs, [field.name]: !structOpen });

  return (
    <div>
      {/* header: name · mod badges · type · null toggle */}
      <div className="mb-1.5 flex items-center gap-2">
        <label className="font-mono text-[13px] font-semibold">
          {field.name}
        </label>
        {field.mods.map((m) => (
          <ModBadge key={m} mod={m} />
        ))}
        <span className="font-mono text-[10px] font-semibold text-muted-foreground">
          {field.typeLabel}
        </span>
        {field.directive ? (
          <span className="font-mono text-[10px] text-muted-foreground/70">
            {field.directive}
          </span>
        ) : null}
        {scalarNullable ? (
          <button
            type="button"
            onClick={toggleNull}
            className={cn(
              "ml-auto rounded border px-2 py-0.5 font-mono text-[11px] transition-colors",
              isNull
                ? "border-primary bg-primary/15 text-primary"
                : "border-border text-muted-foreground hover:bg-muted",
            )}
          >
            {isNull ? "set value" : "set null"}
          </button>
        ) : null}
      </div>

      {/* null state */}
      {scalarNullable && isNull ? (
        <div className="rounded-lg border border-dashed border-border bg-muted/40 px-3 py-2 font-mono text-[12.5px] text-muted-foreground">
          ∅ NULL — value absent (distinct from empty)
        </div>
      ) : null}

      {/* value states (hidden when nulled) */}
      {!(scalarNullable && isNull) ? (
        <>
          {c === "uuid" ? (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/50 px-3 py-2 font-mono text-[12.5px] text-muted-foreground">
              <span className="flex-1 truncate">{field.value}</span>
              <span className="rounded bg-muted px-1.5 py-0.5 text-[10px]">
                system-managed
              </span>
              <button type="button" title="copy" className="hover:text-foreground">
                <Copy className="size-3.5" />
              </button>
            </div>
          ) : null}

          {c === "string" ? (
            <Input
              value={val(field.value)}
              onChange={(e) => onVal(e.target.value)}
              placeholder={field.placeholder}
            />
          ) : null}

          {c === "text" ? (
            <Textarea
              value={val(field.value)}
              onChange={(e) => onVal(e.target.value)}
              rows={3}
            />
          ) : null}

          {c === "int" ? (
            <Input
              type="number"
              value={val(field.value)}
              onChange={(e) => onVal(e.target.value)}
              min={field.min}
              max={field.max}
            />
          ) : null}

          {c === "bigint" ? (
            <>
              <Input value={val(field.value)} onChange={(e) => onVal(e.target.value)} />
              <div className="mt-1.5 flex items-center gap-1.5 text-[11px] text-warn">
                <span>⚠</span>edited as string — u64 exceeds JS safe-integer
                precision
              </div>
            </>
          ) : null}

          {c === "float" ? (
            <Input
              type="number"
              step="any"
              value={val(field.value)}
              onChange={(e) => onVal(e.target.value)}
            />
          ) : null}

          {c === "char" ? (
            <div className="relative">
              <Input
                value={val(field.value)}
                onChange={(e) => onVal(e.target.value)}
                maxLength={field.len}
              />
              <span className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 font-mono text-[10.5px] text-muted-foreground">
                {val(field.value).length}/{field.len} bytes
              </span>
            </div>
          ) : null}

          {c === "ts" ? (
            <div>
              <div className="grid grid-cols-[1.1fr_1fr] gap-2">
                <div>
                  <div className="mb-1 font-mono text-[10px] text-muted-foreground">
                    unix ms
                  </div>
                  <Input
                    value={val(field.msVal)}
                    onChange={(e) => onVal(e.target.value)}
                    disabled={autoGen}
                  />
                </div>
                <div>
                  <div className="mb-1 font-mono text-[10px] text-muted-foreground">
                    human
                  </div>
                  <Input defaultValue={field.humanVal} disabled={autoGen} />
                </div>
              </div>
              {autoGen ? (
                <div className="mt-1.5 text-[11px] text-muted-foreground">
                  auto-generated on insert · read-only
                </div>
              ) : null}
            </div>
          ) : null}
        </>
      ) : null}

      {/* bool tri-state (owns its own null handling) */}
      {c === "bool" ? (
        <div className="flex w-fit gap-1 rounded-lg border border-border bg-muted p-1">
          {(
            [
              ["true", "true"],
              ["false", "false"],
              ...(nullable ? ([["null", "∅ null"]] as const) : []),
            ] as const
          ).map(([v, label]) => (
            <button
              key={v}
              type="button"
              onClick={() => setBools({ ...bools, [field.name]: v })}
              className={cn(
                "rounded-md px-3 py-1 text-xs font-semibold transition-colors",
                boolVal === v
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-background/60",
              )}
            >
              {label}
            </button>
          ))}
        </div>
      ) : null}

      {/* fk picker */}
      {c === "fk" ? (
        <div>
          <div className="flex items-center gap-2">
            <div className="flex-1">
              <Select
                value={values[field.name] ?? field.fkCurrent}
                onValueChange={onVal}
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder={`pick ${field.fkTarget} row`} />
                </SelectTrigger>
                <SelectContent>
                  {field.fkOptions?.map((o) => (
                    <SelectItem key={o.v} value={o.v}>
                      {o.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button
              variant="outline"
              size="icon"
              title="open target"
              onClick={() =>
                field.fkTarget &&
                onFollowRelation(field.fkTarget, field.name)
              }
            >
              <ArrowRight className="size-4" />
            </Button>
          </div>
          <div className="mt-1.5 text-[11px] text-muted-foreground">
            {nullable
              ? "optional FK — can be set to null · UUID-keyed traversal"
              : `required FK — must point to an existing ${field.fkTarget} row`}
          </div>
        </div>
      ) : null}

      {/* struct */}
      {c === "struct" ? (
        structOpen ? (
          <div className="flex flex-col gap-2.5 rounded-lg border border-border bg-muted/30 p-3">
            {field.structFields?.map((sf) => (
              <div key={sf.name}>
                <div className="mb-1 font-mono text-[10px] text-muted-foreground">
                  {sf.name}
                </div>
                <Input defaultValue={sf.value} placeholder={sf.ph} />
              </div>
            ))}
            <button
              type="button"
              onClick={toggleStruct}
              className="self-start text-[12px] text-danger hover:underline"
            >
              Set whole struct to null
            </button>
          </div>
        ) : (
          <div className="flex items-center gap-2.5 rounded-lg border border-dashed border-border px-3 py-2 font-mono text-[12.5px] text-muted-foreground">
            ∅ NULL
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              onClick={toggleStruct}
            >
              Set value
            </Button>
          </div>
        )
      ) : null}

      {/* has-many */}
      {c === "hasmany" ? (
        <div>
          <button
            type="button"
            onClick={() =>
              field.target && onFollowRelation(field.target, field.name)
            }
            className="flex w-full items-center gap-2.5 rounded-lg border border-border bg-card px-3 py-2 text-left hover:bg-muted"
          >
            <span className="rounded border border-border px-1.5 py-px font-mono text-[10px] font-semibold text-muted-foreground">
              has-many
            </span>
            <span className="text-[12.5px]">
              {field.relCount} {field.target} rows
            </span>
            <span className="ml-auto text-muted-foreground">view →</span>
          </button>
          <div className="mt-1.5 text-[11px] text-warn">
            reverse lookup is a linear scan — paginated, may be slow at scale
          </div>
        </div>
      ) : null}

      {/* m2m */}
      {c === "m2m" ? (
        <div>
          <div className="flex flex-wrap items-center gap-1.5">
            {field.chips?.map((chip) => (
              <span
                key={chip.label}
                className="inline-flex items-center gap-1.5 rounded-full border border-info/30 bg-info/15 px-2.5 py-0.5 font-mono text-[12px] text-info"
              >
                {chip.label}
              </span>
            ))}
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-full border border-dashed border-border px-2.5 py-0.5 font-mono text-[12px] text-muted-foreground hover:bg-muted"
            >
              + link {field.target}
            </button>
          </div>
          <div className="mt-1.5 text-[11px] text-muted-foreground">
            links can be added, but not removed — no unlink operation exists
          </div>
        </div>
      ) : null}
    </div>
  );
}
