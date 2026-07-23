import type { Metadata } from "next";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { catalog } from "@/lib/examples";
import { Badge } from "@/components/ui/badge";

export const metadata: Metadata = {
  title: "Examples",
  description:
    "A catalog of realistic .forge application schemas across many domains — every one parses and generates.",
};

export default function ExamplesPage() {
  return (
    <main className="mx-auto max-w-screen-xl px-4 py-14 sm:px-6">
      <header className="mb-10 max-w-2xl">
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Example schemas</h1>
        <p className="mt-3 text-lg text-muted-foreground">
          {catalog.length} realistic <code>.forge</code>{" "}
          applications exercising the schema language across many domains. Some are adapted from
          well-known open-source apps and classic sample databases; others are synthetic. Every one
          passes <code>validate --strict</code> and <code>generate all</code>.
        </p>
      </header>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {catalog.map((ex) => (
          <Link
            key={ex.slug}
            href={`/examples/${ex.slug}/`}
            className="group flex flex-col gap-3 rounded-xl border border-border/60 bg-card/40 p-5 transition-colors hover:border-primary/40 hover:bg-accent/40"
          >
            <div className="flex items-center justify-between gap-2">
              <h2 className="font-semibold">{ex.title}</h2>
              <Badge variant={ex.origin === "Adapted" ? "secondary" : "outline"} className="shrink-0">
                {ex.origin}
              </Badge>
            </div>
            <p className="text-sm text-muted-foreground">{ex.showcases}</p>
            <div className="mt-auto flex items-center justify-between pt-1 text-xs text-muted-foreground">
              <span>
                {ex.models} models · {ex.provenance}
              </span>
              <ArrowRight className="size-3.5 text-primary opacity-0 transition-opacity group-hover:opacity-100" />
            </div>
          </Link>
        ))}
      </div>
    </main>
  );
}
