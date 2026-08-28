import Link from "next/link";
import {
  type LucideIcon,
  ArrowRight,
  ShieldCheck,
  Columns3,
  HardDriveDownload,
  GitBranch,
  Radio,
  Building2,
  Globe,
  History,
  Code2,
  Boxes,
} from "lucide-react";
import { site } from "@/lib/site";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { CodeBlock } from "@/components/code-block";
import { CopyCommand } from "@/components/marketing/copy-command";
import { GitHubIcon } from "@/components/icons";
import { AnimatedForgeMark } from "@/components/animated-forgemark";
import { Markdown } from "@/components/markdown";
import { landing, type IconKey } from "@/content/landing";

const ICONS: Record<IconKey, LucideIcon> = {
  ShieldCheck,
  Columns3,
  HardDriveDownload,
  GitBranch,
  Radio,
  Building2,
  Globe,
  History,
  Code2,
};

export default function Home() {
  return (
    <main>
      { }
      <section className="relative overflow-hidden border-b border-border/50">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(60%_50%_at_50%_-10%,color-mix(in_oklch,var(--primary)_22%,transparent),transparent)]"
        />
        <div className="mx-auto max-w-screen-xl px-4 py-16 text-center sm:px-6 sm:pt-18 sm:pb-26">
          { }
          <AnimatedForgeMark className="mx-auto mb-8 size-40 sm:mb-10 sm:size-56 lg:mb-11 lg:size-64" />
          <Markdown
            as="h1"
            inline
            className="mx-auto max-w-4xl text-balance text-4xl font-bold tracking-tight sm:text-6xl"
            source={landing.hero.heading}
            contentKey="hero.heading"
          />
          <Markdown
            as="p"
            inline
            className="mx-auto mt-6 max-w-2xl text-balance text-lg text-muted-foreground sm:text-xl"
            source={landing.hero.subhead}
            contentKey="hero.subhead"
          />
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <Link href="/docs/quickstart/">
                <Markdown inline source={landing.hero.ctaPrimary} contentKey="hero.ctaPrimary" />
                <ArrowRight className="size-4" />
              </Link>
            </Button>
            <Button asChild size="lg" variant="outline">
              <a href={site.github} target="_blank" rel="noreferrer noopener">
                <GitHubIcon className="size-4" />
                <Markdown inline source={landing.hero.ctaGithub} contentKey="hero.ctaGithub" />
              </a>
            </Button>
          </div>
          <div className="mt-6 flex flex-col items-center gap-2.5">
            <CopyCommand command={landing.hero.install} />
            <Link
              href="/docs/installation/"
              className="text-sm text-muted-foreground underline-offset-4 transition-colors hover:text-foreground hover:underline"
            >
              <Markdown inline source={landing.hero.installMore} contentKey="hero.installMore" />
            </Link>
          </div>
        </div>
      </section>
      { }
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mx-auto mb-10 max-w-2xl text-center">
          <Markdown
            as="h2"
            inline
            className="text-3xl font-semibold tracking-tight"
            source={landing.showcase.heading}
            contentKey="showcase.heading"
          />
          <Markdown
            as="p"
            inline
            className="mt-3 text-muted-foreground"
            source={landing.showcase.body}
            contentKey="showcase.body"
          />
        </div>
        <CodeBlock
          className="mx-auto max-w-3xl"
          code={landing.showcase.schema}
          lang="forge"
          filename="schema.forge"
        />
      </section>
      { }
      <section className="border-y border-border/50 bg-muted/20">
        <div className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
          <div className="mx-auto mb-10 max-w-2xl text-center">
            <Markdown
              as="h2"
              inline
              className="text-3xl font-semibold tracking-tight"
              source={landing.clients.heading}
              contentKey="clients.heading"
            />
            <Markdown
              as="p"
              inline
              className="mt-3 text-muted-foreground"
              source={landing.clients.body}
              contentKey="clients.body"
            />
          </div>
          <Tabs defaultValue={landing.clients.tabs[0]?.id} className="mx-auto w-full max-w-3xl">
            <TabsList className="self-center">
              {landing.clients.tabs.map((t) => (
                <TabsTrigger key={t.id} value={t.id}>
                  {t.label}
                </TabsTrigger>
              ))}
            </TabsList>
            {landing.clients.tabs.map((t) => (
              <TabsContent key={t.id} value={t.id} className="mt-4">
                <CodeBlock code={t.code} lang={t.lang} filename={t.filename} />
              </TabsContent>
            ))}
          </Tabs>
          <Markdown
            as="p"
            inline
            className="mx-auto mt-6 max-w-2xl text-center text-sm text-muted-foreground"
            source={landing.clients.note}
            contentKey="clients.note"
          />
        </div>
      </section>
      { }
      <section className="border-y border-border/50 bg-muted/20">
        <div className="mx-auto max-w-screen-lg px-4 py-14 text-center sm:px-6">
          <Markdown
            as="p"
            inline
            className="text-lg font-medium sm:text-2xl"
            source={landing.invariant.lead}
            contentKey="invariant.lead"
          />
          <Markdown
            as="p"
            inline
            className="mx-auto mt-4 max-w-2xl text-muted-foreground"
            source={landing.invariant.body}
            contentKey="invariant.body"
          />
        </div>
      </section>
      { }
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mb-10 max-w-2xl">
          <Markdown
            as="h2"
            inline
            className="text-3xl font-semibold tracking-tight"
            source={landing.features.heading}
            contentKey="features.heading"
          />
          <Markdown
            as="p"
            inline
            className="mt-3 text-muted-foreground"
            source={landing.features.body}
            contentKey="features.body"
          />
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {landing.features.items.map((f, i) => {
            const Icon = ICONS[f.icon];
            return (
              <Link
                key={f.href}
                href={f.href}
                className="group flex flex-col gap-3 rounded-xl border border-border/60 bg-card/40 p-5 transition-colors hover:border-primary/40 hover:bg-accent/40"
              >
                <Icon className="size-5 text-primary" />
                <Markdown
                  as="h3"
                  inline
                  className="font-semibold"
                  source={f.title}
                  contentKey={`features.items.${i}.title`}
                />
                <Markdown
                  as="p"
                  inline
                  className="text-sm text-muted-foreground"
                  source={f.body}
                  contentKey={`features.items.${i}.body`}
                />
                <span className="mt-auto inline-flex items-center gap-1 pt-1 text-sm text-primary opacity-0 transition-opacity group-hover:opacity-100">
                  <Markdown inline source={landing.features.learnMore} contentKey="features.learnMore" />
                  <ArrowRight className="size-3.5" />
                </span>
              </Link>
            );
          })}
        </div>
      </section>
      { }
      <section className="border-y border-border/50 bg-muted/20">
        <div className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
          <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
            <div className="max-w-xl">
              <Markdown
                as="h2"
                inline
                className="text-3xl font-semibold tracking-tight"
                source={landing.stats.heading}
                contentKey="stats.heading"
              />
              <Markdown
                as="p"
                inline
                className="mt-3 text-muted-foreground"
                source={landing.stats.body}
                contentKey="stats.body"
              />
            </div>
            <Button asChild variant="outline">
              <Link href="/docs/benchmarks/">
                <Markdown inline source={landing.stats.cta} contentKey="stats.cta" />
                <ArrowRight className="size-4" />
              </Link>
            </Button>
          </div>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {landing.stats.items.map((s, i) => (
              <div key={i} className="rounded-xl border border-border/60 bg-background/40 p-5">
                <Markdown
                  as="div"
                  inline
                  className="text-2xl font-bold tracking-tight text-primary"
                  source={s.value}
                  contentKey={`stats.items.${i}.value`}
                />
                <Markdown
                  as="div"
                  inline
                  className="mt-1.5 text-sm text-muted-foreground"
                  source={s.label}
                  contentKey={`stats.items.${i}.label`}
                />
              </div>
            ))}
          </div>
        </div>
      </section>
      { }
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mb-10 max-w-2xl">
          <Markdown
            as="h2"
            inline
            className="text-3xl font-semibold tracking-tight"
            source={landing.steps.heading}
            contentKey="steps.heading"
          />
          <Markdown
            as="p"
            inline
            className="mt-3 text-muted-foreground"
            source={landing.steps.body}
            contentKey="steps.body"
          />
        </div>
        <div className="grid gap-6 lg:grid-cols-3">
          {landing.steps.items.map((s, i) => (
            <div key={s.n} className="flex flex-col gap-4">
              <div className="flex items-center gap-3">
                <span className="font-mono text-sm text-primary">{s.n}</span>
                <Markdown
                  as="h3"
                  inline
                  className="text-lg font-semibold"
                  source={s.title}
                  contentKey={`steps.items.${i}.title`}
                />
              </div>
              <Markdown
                as="p"
                inline
                className="text-sm text-muted-foreground"
                source={s.body}
                contentKey={`steps.items.${i}.body`}
              />
              <CodeBlock code={s.code} lang={s.lang} />
            </div>
          ))}
        </div>
      </section>
      { }
      <section className="border-t border-border/50">
        <div className="mx-auto max-w-screen-lg px-4 py-20 text-center sm:px-6">
          <Boxes className="mx-auto mb-5 size-8 text-primary" />
          <Markdown
            as="h2"
            inline
            className="text-3xl font-semibold tracking-tight sm:text-4xl"
            source={landing.cta.heading}
            contentKey="cta.heading"
          />
          <Markdown
            as="p"
            inline
            className="mx-auto mt-4 max-w-xl text-muted-foreground"
            source={landing.cta.body}
            contentKey="cta.body"
          />
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <Link href="/docs/quickstart/">
                <Markdown inline source={landing.cta.primary} contentKey="cta.primary" />
                <ArrowRight className="size-4" />
              </Link>
            </Button>
            <Button asChild size="lg" variant="outline">
              <Link href="/docs/">
                <Markdown inline source={landing.cta.secondary} contentKey="cta.secondary" />
              </Link>
            </Button>
          </div>
        </div>
      </section>
    </main>
  );
}
