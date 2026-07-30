import type { Metadata } from "next";
import { ChevronDown } from "lucide-react";
import { getReleases, releaseAnchor } from "@/lib/changelog";
import { site } from "@/lib/site";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export const dynamic = "error";

export const metadata: Metadata = {
  title: "Changelog",
  description:
    "What shipped in each ForgeDB release — features, fixes, and performance, generated from the project's conventional commit history.",
};

const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** Format "YYYY-MM-DD" without constructing a Date (avoids build-env TZ drift). */
function formatDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return iso;
  const [, y, mo, d] = m;
  const month = MONTHS[Number(mo) - 1] ?? mo;
  return `${month} ${Number(d)}, ${y}`;
}

// The generated changes carry inline HTML (### group headings + bullet lists).
// Style them here via scoped arbitrary variants so the changelog reads tight
// without pulling in the docs' heavier MDX component set. Inline `code` inherits
// the global chip (`code:not(pre code)` in globals.css).
const BODY_PROSE = cn(
  "[&_h3]:mt-6 [&_h3]:mb-2 [&_h3]:text-xs [&_h3]:font-semibold [&_h3]:uppercase [&_h3]:tracking-wider [&_h3]:text-muted-foreground [&_h3:first-child]:mt-0",
  "[&_ul]:mb-5 [&_ul]:list-disc [&_ul]:space-y-1.5 [&_ul]:pl-5 [&_ul:last-child]:mb-0",
  "[&_li]:text-[15px] [&_li]:leading-relaxed [&_li]:text-foreground/90 [&_li]:marker:text-muted-foreground/40",
  "[&_a]:font-medium [&_a]:text-primary [&_a]:no-underline hover:[&_a]:underline [&_a]:underline-offset-4",
  "[&_strong]:font-semibold [&_strong]:text-foreground",
);

export default function ChangelogPage() {
  const releases = getReleases();

  return (
    <main className="mx-auto max-w-3xl px-4 py-14 sm:px-6">
      <header className="mb-8">
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Changelog</h1>
        <p className="mt-3 text-lg text-muted-foreground">
          What shipped in each release of the ForgeDB core — the <code>forgedb</code> CLI and its
          published substrate crates. Generated from the project&apos;s conventional commit history.
        </p>
        <p className="mt-3 text-sm text-muted-foreground">
          The VS Code extension and packaging channels ship on their own lines and are not listed
          here. For downloadable artifacts and signed release notes, see the{" "}
          <a
            href={`${site.github}/releases`}
            target="_blank"
            rel="noreferrer noopener"
            className="font-medium text-primary underline-offset-4 hover:underline"
          >
            GitHub releases
          </a>
          .
        </p>
      </header>

      {releases.length === 0 ? (
        <p className="rounded-lg border border-border/60 bg-muted/20 px-4 py-6 text-sm text-muted-foreground">
          No releases have been published yet. Check back after the first tagged release.
        </p>
      ) : (
        <div className="divide-y divide-border/60">
          {releases.map((r, i) => (
            <details
              key={r.version}
              id={releaseAnchor(r)}
              open={i === 0}
              className="group scroll-mt-20 py-6 first:pt-0 last:pb-0"
            >
              <summary className="flex cursor-pointer list-none items-center gap-3 [&::-webkit-details-marker]:hidden">
                <h2 className="font-mono text-lg font-semibold tracking-tight">
                  {r.unreleased ? "Unreleased" : `v${r.version}`}
                </h2>
                {r.date ? (
                  <time className="text-sm text-muted-foreground">{formatDate(r.date)}</time>
                ) : (
                  <Badge variant="outline" className="text-[11px]">
                    Pending release
                  </Badge>
                )}
                <span className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
                  {r.count} {r.count === 1 ? "change" : "changes"}
                  <ChevronDown className="size-4 transition-transform group-open:rotate-180" />
                </span>
              </summary>
              <div
                className={cn("mt-5", BODY_PROSE)}
                dangerouslySetInnerHTML={{ __html: r.html }}
              />
            </details>
          ))}
        </div>
      )}
    </main>
  );
}
