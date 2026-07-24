import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { MDXRemote } from "next-mdx-remote/rsc";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import rehypePrettyCode from "rehype-pretty-code";
import {
  getAllDocSlugs,
  getDocBySlug,
  hrefForSlug,
  isDetailedSlug,
  hasDetailedVariant,
  DETAILED_SEGMENT,
} from "@/lib/mdx";
import { extractToc } from "@/lib/toc";
import { docMeta } from "@/lib/docs-nav";
import { rehypePrettyCodeOptions } from "@/lib/rehype-code";
import { remarkSourceMap } from "@/lib/dev/remark-source-map";
import { hashContent } from "@/lib/dev/rewrite-hash";
import { mdxComponents } from "@/components/mdx/mdx-components";
import { DetailToggle } from "@/components/docs/detail-toggle";
import { VariantToggle } from "@/components/docs/variant-toggle";
import { Toc } from "@/components/docs/toc";
import { DocsPager } from "@/components/docs/pager";
import { DocsBreadcrumb } from "@/components/docs/breadcrumb";

// Fully static: enumerate every doc page at build time.
export const dynamic = "error";
export const dynamicParams = false;

type Params = { slug?: string[] };

export function generateStaticParams(): Params[] {
  // The optional catch-all needs the index represented as an empty slug.
  return getAllDocSlugs().map((slug) => ({ slug: slug.length ? slug : [] }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<Params>;
}): Promise<Metadata> {
  const { slug = [] } = await params;
  const doc = getDocBySlug(slug);
  if (!doc) return {};
  const detailed = isDetailedSlug(slug);
  const baseHref = detailed ? hrefForSlug(slug.slice(0, -1)) : doc.href;
  return {
    title: detailed ? `${doc.frontmatter.title} — detailed` : doc.frontmatter.title,
    description: doc.frontmatter.description,
    // The terse body is canonical; a detailed variant points its canonical at it
    // so the two tellings don't compete as duplicate content.
    ...(detailed ? { alternates: { canonical: baseHref } } : {}),
  };
}

export default async function DocPage({ params }: { params: Promise<Params> }) {
  const { slug = [] } = await params;
  const doc = getDocBySlug(slug);
  if (!doc) notFound();

  const toc = extractToc(doc.content);

  // Build-C pages keep terse + detailed as sibling routes. Page chrome
  // (breadcrumb group, prev/next) always comes from the base page so a detailed
  // variant sits in the same place in the docs as its terse twin.
  const detailed = isDetailedSlug(slug);
  const baseSlug = detailed ? slug.slice(0, -1) : slug;
  const baseHref = hrefForSlug(baseSlug);
  const meta = docMeta(baseHref);
  const isCPage = detailed || hasDetailedVariant(baseSlug);

  // Only surface the verbosity control on pages that actually have deeper blocks
  // to expand — otherwise it would toggle nothing. (Build-C pages use the
  // variant switch instead, which is always shown.)
  const hasTiers = /<(DiveDeeper|ImplementationDetails)\b/.test(doc.content);

  return (
    <div className="flex gap-8">
      <article className="min-w-0 flex-1 py-8 lg:max-w-3xl">
        <DocsBreadcrumb group={meta.group} title={doc.frontmatter.title} />
        <header className="mb-6">
          <h1 className="scroll-m-20 text-3xl font-bold tracking-tight sm:text-4xl">
            {doc.frontmatter.title}
          </h1>
          {doc.frontmatter.description ? (
            <p className="mt-2 text-lg text-muted-foreground">{doc.frontmatter.description}</p>
          ) : null}
          {isCPage ? (
            <div className="mt-4 flex justify-end">
              <VariantToggle
                terseHref={baseHref}
                detailedHref={hrefForSlug([...baseSlug, DETAILED_SEGMENT])}
                active={detailed ? "detailed" : "terse"}
              />
            </div>
          ) : hasTiers ? (
            <div className="mt-4 flex justify-end">
              <DetailToggle />
            </div>
          ) : null}
        </header>

        {process.env.NODE_ENV === "development" ? (
          // Staleness fingerprint for the local prose-rewrite tool: if the .mdx
          // body changes after this render, stamped offsets are stale and a
          // splice is refused server-side. Never emitted in the export build.
          <span hidden data-rewrite-doc-hash={hashContent(doc.content)} />
        ) : null}

        <div className="text-[15px]">
          <MDXRemote
            source={doc.content}
            components={mdxComponents}
            options={{
              mdxOptions: {
                // Dev-only: stamp source offsets onto blocks so the in-browser
                // rewrite tool can map a rendered element back to the .mdx.
                // Never added to the production export build.
                remarkPlugins:
                  process.env.NODE_ENV === "development"
                    ? [remarkGfm, remarkSourceMap]
                    : [remarkGfm],
                rehypePlugins: [rehypeSlug, [rehypePrettyCode, rehypePrettyCodeOptions]],
              },
            }}
          />
        </div>

        <DocsPager prev={meta.prev} next={meta.next} />
      </article>
      <Toc entries={toc} />
    </div>
  );
}
