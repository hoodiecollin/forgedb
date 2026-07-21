import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { catalog, getExample } from "@/lib/examples";
import { CodeBlock } from "@/components/code-block";
import { Badge } from "@/components/ui/badge";

export const dynamicParams = false;

export function generateStaticParams() {
  return catalog.map((e) => ({ slug: e.slug }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const ex = getExample(slug);
  if (!ex) return {};
  return { title: `${ex.title} example`, description: ex.showcases };
}

export default async function ExampleDetail({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const ex = getExample(slug);
  if (!ex) notFound();

  return (
    <main className="mx-auto max-w-4xl px-4 py-12 sm:px-6">
      <Link
        href="/examples/"
        className="mb-6 inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
      >
        <ArrowLeft className="size-3.5" /> All examples
      </Link>

      <header className="mb-6">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <h1 className="text-3xl font-bold tracking-tight">{ex.title}</h1>
          <Badge variant={ex.origin === "Adapted" ? "secondary" : "outline"}>{ex.origin}</Badge>
        </div>
        <p className="text-lg text-muted-foreground">{ex.showcases}</p>
        <p className="mt-2 text-sm text-muted-foreground">
          {ex.models} models · {ex.provenance}
        </p>
      </header>

      <div className="mb-6 rounded-lg border border-border/60 bg-muted/30 p-4 font-mono text-sm">
        <span className="text-muted-foreground"># generate this app</span>
        <br />
        <span className="text-primary">$</span> cd examples/{ex.slug} && forgedb generate all --output ./out
      </div>

      <CodeBlock code={ex.source} lang="forge" filename={`examples/${ex.slug}/schema.forge`} />
    </main>
  );
}
