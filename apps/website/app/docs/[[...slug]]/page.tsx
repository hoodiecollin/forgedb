import type { Metadata } from "next";
import { Suspense } from "react";
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
import { EcosystemToggle } from "@/components/docs/ecosystem-toggle";
import { Toc } from "@/components/docs/toc";
import { DocsPager } from "@/components/docs/pager";
import { DocsBreadcrumb } from "@/components/docs/breadcrumb";

export const dynamic = "error";
export const dynamicParams = false;

type Params = { slug?: string[] };

export function generateStaticParams(): Params[] {
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
    ...(detailed ? { alternates: { canonical: baseHref } } : {}),
  };
}

export default async function DocPage({ params }: { params: Promise<Params> }) {
  const { slug = [] } = await params;
  const doc = getDocBySlug(slug);
  if (!doc) notFound();

  const toc = extractToc(doc.content);

  const detailed = isDetailedSlug(slug);
  const baseSlug = detailed ? slug.slice(0, -1) : slug;
  const baseHref = hrefForSlug(baseSlug);
  const meta = docMeta(baseHref);
  const isCPage = detailed || hasDetailedVariant(baseSlug);

  const hasTiers = /<(DiveDeeper|ImplementationDetails)\b/.test(doc.content);

  const hasEco = /<Eco\b/.test(doc.content);

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
          {isCPage || hasTiers || hasEco ? (
            <div className="mt-4 flex flex-wrap items-center justify-end gap-2">
              {hasEco ? (
                <Suspense fallback={null}>
                  <EcosystemToggle />
                </Suspense>
              ) : null}
              {isCPage ? (
                <VariantToggle
                  terseHref={baseHref}
                  detailedHref={hrefForSlug([...baseSlug, DETAILED_SEGMENT])}
                  active={detailed ? "detailed" : "terse"}
                />
              ) : hasTiers ? (
                <DetailToggle />
              ) : null}
            </div>
          ) : null}
        </header>

        {process.env.NODE_ENV === "development" ? (
          <span hidden data-rewrite-doc-hash={hashContent(doc.content)} />
        ) : null}

        <div className="text-[15px]">
          <MDXRemote
            source={doc.content}
            components={mdxComponents}
            options={{
              mdxOptions: {
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
