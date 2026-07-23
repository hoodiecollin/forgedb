import Link from "next/link";
import {
  ArrowRight,
  ShieldCheck,
  Columns3,
  HardDriveDownload,
  GitBranch,
  Radio,
  Building2,
  Archive,
  Globe,
  History,
  Boxes,
  Zap,
} from "lucide-react";
import { site } from "@/lib/site";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { CodeBlock } from "@/components/code-block";
import { CopyCommand } from "@/components/marketing/copy-command";
import { GitHubIcon, ForgeMark } from "@/components/icons";

const SCHEMA = `// schema.forge — the single source of truth
User {
  id: +uuid
  name: string @length(1, 100)
  email: &string @email
  created_at: +timestamp
  posts: [Post]
}

Post {
  id: +uuid
  title: string @length(1, 200)
  slug: &string
  views: ^u64
  published: bool
  author: *User
  created_at: +timestamp

  @projection(card: title, slug, views)
}`;

const SDK = `// generated TypeScript SDK — fully typed from your schema
import { ForgeDBClient } from "./generated/types";

const db = new ForgeDBClient("http://localhost:3000");

// createPost() is typed to PostCreate; returns the new id
const id = await db.createPost({
  title: "Hello, ForgeDB",
  slug: "hello-forgedb",
  views: 0,
  published: true,
  author: userId,
});

// listPost() → ListResult<Post> { data, total, limit, offset }
const { data, total } = await db.listPost({
  filter: { published: true },
  sort: "views",
  order: "desc",
  limit: 10,
});`;

const features: {
  icon: typeof ShieldCheck;
  title: string;
  body: string;
  href: string;
}[] = [
  {
    icon: ShieldCheck,
    title: "Compile-time type safety",
    body: "Your schema becomes tailored Rust and TypeScript types. Schema drift is impossible — the types are generated, not reflected at runtime.",
    href: "/docs/concepts/",
  },
  {
    icon: Columns3,
    title: "Columnar storage",
    body: "Fixed-width and variable-length columns with a tiny on-disk footprint and column-pruned scans generated per model.",
    href: "/docs/features/indexes/",
  },
  {
    icon: HardDriveDownload,
    title: "Crash-safe durable writes",
    body: "Every write hits a per-model WAL with an fsync barrier before columns, with torn-tail recovery and a single-writer lock.",
    href: "/docs/features/durability/",
  },
  {
    icon: GitBranch,
    title: "Transactions & MVCC",
    body: "Atomic generated transactions, optimistic concurrent writers, and a multi-process commit coordinator — three strict tiers.",
    href: "/docs/features/transactions-mvcc/",
  },
  {
    icon: Radio,
    title: "Live queries & change feed",
    body: "Subscribe over WebSockets to typed inserts, updates, and removals — result-set deltas generated from the same filters as your REST API.",
    href: "/docs/features/live-queries/",
  },
  {
    icon: Building2,
    title: "Multi-tenancy & auth",
    body: "Physical dir-per-tenant isolation with a verify-only JWT tenant guard — process-per-tenant, generated code stays tenant-oblivious.",
    href: "/docs/features/multi-tenancy/",
  },
  {
    icon: Globe,
    title: "Browser read-replica",
    body: "Compile the same generated database to WASM and follow a durable replication stream — query a local replica in the browser over IndexedDB or OPFS.",
    href: "/docs/features/browser-replica/",
  },
  {
    icon: History,
    title: "Point-in-time reads",
    body: "Lock-free watermark snapshots with zero version machinery — read the database as of an earlier commit over the same REST surface.",
    href: "/docs/features/snapshot-reads/",
  },
  {
    icon: Archive,
    title: "Backup & migrations",
    body: "Lock-free full-snapshot backup/restore, plus a version-guarded migration workflow with an offline data transformer.",
    href: "/docs/features/migrations/",
  },
];

const stats: { value: string; label: string }[] = [
  { value: "1 schema", label: "→ Rust DB + TS SDK + REST API + OpenAPI" },
  { value: "1.88 MB", label: "on-disk footprint — smallest of the embedded four" },
  { value: "0.37 s", label: "group-commit bulk load of 10k rows" },
  { value: "0 deps", label: "on any runtime ORM — only schema-agnostic substrate" },
];

const steps: { n: string; title: string; body: string; code: string; lang: string }[] = [
  {
    n: "01",
    title: "Write a schema",
    body: "Describe your models, relations, and constraints in a declarative .forge file.",
    code: "User {\n  id: +uuid\n  email: &string @email\n  posts: [Post]\n}",
    lang: "forge",
  },
  {
    n: "02",
    title: "Generate",
    body: "Transpile the schema into tailored Rust, a TypeScript SDK, a REST API, and an OpenAPI spec.",
    code: "forgedb generate all --output ./generated",
    lang: "bash",
  },
  {
    n: "03",
    title: "Build & serve",
    body: "Compile the generated crate and run the server — a real REST API over columnar storage.",
    code: "forgedb build && ./target/release/server",
    lang: "bash",
  },
];

export default function Home() {
  return (
    <main>
      {/* Hero */}
      <section className="relative overflow-hidden border-b border-border/50">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(60%_50%_at_50%_-10%,color-mix(in_oklch,var(--primary)_22%,transparent),transparent)]"
        />
        <div className="mx-auto max-w-screen-xl px-4 py-20 text-center sm:px-6 sm:py-28">
          <ForgeMark className="mx-auto mb-6 size-16" />
          <Badge variant="secondary" className="mb-5 gap-1.5 rounded-full px-3 py-1">
            <Zap className="size-3.5 text-primary" />
            Schema-first · compile-time · pre-1.0
          </Badge>
          <h1 className="mx-auto max-w-4xl text-balance text-4xl font-bold tracking-tight sm:text-6xl">
            The application-database{" "}
            <span className="text-primary">generator</span>
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-balance text-lg text-muted-foreground sm:text-xl">
            Write one <code>.forge</code>{" "}
            schema. ForgeDB compiles it into a tailored Rust database, a TypeScript SDK, a REST
            API, and an OpenAPI spec — specialized to your schema at compile time. Not a runtime ORM.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <Link href="/docs/quickstart/">
                Get started <ArrowRight className="size-4" />
              </Link>
            </Button>
            <Button asChild size="lg" variant="outline">
              <a href={site.github} target="_blank" rel="noreferrer noopener">
                <GitHubIcon className="size-4" /> GitHub
              </a>
            </Button>
          </div>
          <div className="mt-6 flex justify-center">
            <CopyCommand command="cargo install forgedb" />
          </div>
        </div>
      </section>

      {/* Code showcase */}
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mx-auto mb-10 max-w-2xl text-center">
          <h2 className="text-3xl font-semibold tracking-tight">One schema in, a full stack out</h2>
          <p className="mt-3 text-muted-foreground">
            The schema-specific surface — types, queries, filters, relations, routes — is generated
            and tailored per app. No generic engine reflects your schema at runtime.
          </p>
        </div>
        <div className="grid items-start gap-4 lg:grid-cols-2">
          <CodeBlock code={SCHEMA} lang="forge" filename="schema.forge" />
          <CodeBlock code={SDK} lang="typescript" filename="app.ts" />
        </div>
      </section>

      {/* Invariant band */}
      <section className="border-y border-border/50 bg-muted/20">
        <div className="mx-auto max-w-screen-lg px-4 py-14 text-center sm:px-6">
          <p className="text-lg font-medium sm:text-2xl">
            The invariant: your schema is a{" "}
            <span className="text-primary">compile-time input to generation</span>, never a runtime
            input to a generic engine.
          </p>
          <p className="mx-auto mt-4 max-w-2xl text-muted-foreground">
            Generated code links only schema-agnostic substrate crates — storage, WAL, change feed,
            auth — that know nothing about any specific schema. The tailored data logic stays
            generated.{" "}
            <Link href="/docs/concepts/" className="text-primary underline-offset-4 hover:underline">
              Read the concepts →
            </Link>
          </p>
        </div>
      </section>

      {/* Feature grid */}
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mb-10 max-w-2xl">
          <h2 className="text-3xl font-semibold tracking-tight">Batteries generated in</h2>
          <p className="mt-3 text-muted-foreground">
            Durability, concurrency, real-time, and multi-tenancy — generated per schema over a
            small, published substrate. Each is honest about its limits.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f) => (
            <Link
              key={f.title}
              href={f.href}
              className="group flex flex-col gap-3 rounded-xl border border-border/60 bg-card/40 p-5 transition-colors hover:border-primary/40 hover:bg-accent/40"
            >
              <f.icon className="size-5 text-primary" />
              <h3 className="font-semibold">{f.title}</h3>
              <p className="text-sm text-muted-foreground">{f.body}</p>
              <span className="mt-auto inline-flex items-center gap-1 pt-1 text-sm text-primary opacity-0 transition-opacity group-hover:opacity-100">
                Learn more <ArrowRight className="size-3.5" />
              </span>
            </Link>
          ))}
        </div>
      </section>

      {/* Stats / benchmarks */}
      <section className="border-y border-border/50 bg-muted/20">
        <div className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
          <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
            <div className="max-w-xl">
              <h2 className="text-3xl font-semibold tracking-tight">Small, fast, and honest</h2>
              <p className="mt-3 text-muted-foreground">
                Benchmarked fairly — matched durability across engines. At the fsync-barrier tier
                ForgeDB ties SQLite and redb; relaxed, it&apos;s the fastest of the group.
              </p>
            </div>
            <Button asChild variant="outline">
              <Link href="/docs/reference/benchmarks/">
                See the numbers <ArrowRight className="size-4" />
              </Link>
            </Button>
          </div>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {stats.map((s) => (
              <div
                key={s.label}
                className="rounded-xl border border-border/60 bg-background/40 p-5"
              >
                <div className="text-2xl font-bold tracking-tight text-primary">{s.value}</div>
                <div className="mt-1.5 text-sm text-muted-foreground">{s.label}</div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* How it works */}
      <section className="mx-auto max-w-screen-xl px-4 py-16 sm:px-6">
        <div className="mb-10 max-w-2xl">
          <h2 className="text-3xl font-semibold tracking-tight">From schema to server</h2>
          <p className="mt-3 text-muted-foreground">
            The whole workflow is three commands. Everything else is generated.
          </p>
        </div>
        <div className="grid gap-6 lg:grid-cols-3">
          {steps.map((s) => (
            <div key={s.n} className="flex flex-col gap-4">
              <div className="flex items-center gap-3">
                <span className="font-mono text-sm text-primary">{s.n}</span>
                <h3 className="text-lg font-semibold">{s.title}</h3>
              </div>
              <p className="text-sm text-muted-foreground">{s.body}</p>
              <CodeBlock code={s.code} lang={s.lang} />
            </div>
          ))}
        </div>
      </section>

      {/* CTA band */}
      <section className="border-t border-border/50">
        <div className="mx-auto max-w-screen-lg px-4 py-20 text-center sm:px-6">
          <Boxes className="mx-auto mb-5 size-8 text-primary" />
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            Generate your database
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-muted-foreground">
            Install the CLI, write a schema, and ship a typed full-stack database in minutes.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <Link href="/docs/quickstart/">
                Start the quickstart <ArrowRight className="size-4" />
              </Link>
            </Button>
            <Button asChild size="lg" variant="outline">
              <Link href="/docs/">Browse the docs</Link>
            </Button>
          </div>
        </div>
      </section>
    </main>
  );
}
