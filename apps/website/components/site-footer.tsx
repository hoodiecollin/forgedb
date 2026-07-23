import Link from "next/link";
import { site } from "@/lib/site";
import { ForgeMark, ForgeWordmark, GitHubIcon } from "@/components/icons";

const cols: { title: string; links: { title: string; href: string }[] }[] = [
  {
    title: "Docs",
    links: [
      { title: "What is ForgeDB", href: "/docs/" },
      { title: "Quickstart", href: "/docs/quickstart/" },
      { title: "Schema language", href: "/docs/schema/overview/" },
      { title: "CLI reference", href: "/docs/cli/overview/" },
    ],
  },
  {
    title: "Features",
    links: [
      { title: "Durability", href: "/docs/features/durability/" },
      { title: "Transactions & MVCC", href: "/docs/features/transactions-mvcc/" },
      { title: "Live queries", href: "/docs/features/live-queries/" },
      { title: "Browser replica", href: "/docs/features/browser-replica/" },
    ],
  },
  {
    title: "Reference",
    links: [
      { title: "Configuration", href: "/docs/config/overview/" },
      { title: "Benchmarks", href: "/docs/benchmarks/" },
      { title: "Deployment", href: "/docs/reference/deployment/" },
      { title: "Versioning", href: "/docs/reference/semver/" },
    ],
  },
];

export function SiteFooter() {
  return (
    <footer className="border-t border-border/60 bg-muted/20">
      <div className="mx-auto grid max-w-screen-2xl gap-10 px-4 py-12 sm:px-6 md:grid-cols-[1.5fr_repeat(3,1fr)]">
        <div className="space-y-3">
          <Link href="/" className="flex items-center gap-2">
            <ForgeMark className="size-6" />
            <ForgeWordmark />
          </Link>
          <p className="max-w-xs text-sm text-muted-foreground">{site.description}</p>
          <div className="flex items-center gap-3 pt-1">
            <a
              href={site.github}
              target="_blank"
              rel="noreferrer noopener"
              className="text-muted-foreground transition-colors hover:text-foreground"
              aria-label="GitHub"
            >
              <GitHubIcon className="size-5" />
            </a>
          </div>
        </div>
        {cols.map((col) => (
          <div key={col.title} className="space-y-3">
            <h3 className="text-sm font-medium">{col.title}</h3>
            <ul className="space-y-2 text-sm text-muted-foreground">
              {col.links.map((l) => (
                <li key={l.href}>
                  <Link href={l.href} className="transition-colors hover:text-foreground">
                    {l.title}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      <div className="border-t border-border/60">
        <div className="mx-auto flex max-w-screen-2xl flex-col gap-1 px-4 py-6 text-xs text-muted-foreground sm:flex-row sm:items-center sm:justify-between sm:px-6">
          <p>Dual-licensed MIT / Apache-2.0.</p>
          <p>Built with ForgeDB's own house stack — Next.js, Tailwind, shadcn/ui.</p>
        </div>
      </div>
    </footer>
  );
}
