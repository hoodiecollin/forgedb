import { dd } from "@/lib/dd";

/**
 * Landing-page copy, extracted from the layout so it can be edited (and, phase 2,
 * fine-tuned in-browser by the rewrite tool) independently of `app/page.tsx`.
 *
 * Prose values are **markdown/MDX** — inline `<code>`, `[links](/href)`, and the
 * custom `<Hl>` primary-highlight — rendered through `components/markdown.tsx`.
 * Code samples are plain source fed to `<CodeBlock>`. Structural tokens (icon
 * keys, hrefs, langs) stay as plain strings; icons resolve to components in the
 * page via the `ICONS` map keyed by `IconKey`.
 *
 * Every multi-line value is authored with `dd` (see `lib/dd.ts`) — no `+`
 * concatenation, no `.join("\n")`.
 */

/** Icon keys the feature grid references; the page maps these to lucide icons. */
export type IconKey =
  | "ShieldCheck"
  | "Columns3"
  | "HardDriveDownload"
  | "GitBranch"
  | "Radio"
  | "Building2"
  | "Globe"
  | "History"
  | "Archive";

export interface FeatureItem {
  icon: IconKey;
  href: string;
  /** markdown */
  title: string;
  /** markdown */
  body: string;
}

export interface StatItem {
  /** markdown */
  value: string;
  /** markdown */
  label: string;
}

export interface StepItem {
  n: string;
  /** markdown */
  title: string;
  /** markdown */
  body: string;
  /** code sample (plain source, not markdown) */
  code: string;
  lang: string;
}

export interface LandingCopy {
  hero: {
    badge: string;
    heading: string;
    subhead: string;
    ctaPrimary: string;
    ctaGithub: string;
    install: string;
  };
  showcase: {
    heading: string;
    body: string;
    schema: string;
    sdk: string;
  };
  invariant: {
    lead: string;
    body: string;
  };
  features: {
    heading: string;
    body: string;
    learnMore: string;
    items: FeatureItem[];
  };
  stats: {
    heading: string;
    body: string;
    cta: string;
    items: StatItem[];
  };
  steps: {
    heading: string;
    body: string;
    items: StepItem[];
  };
  cta: {
    heading: string;
    body: string;
    primary: string;
    secondary: string;
  };
}

export const landing = {
  hero: {
    badge: dd`Schema-first · compile-time · pre-1.0`,
    heading: dd`The application-database <Hl>generator</Hl>`,
    subhead: dd`
      Write one <code>.forge</code> schema. ForgeDB compiles it into a tailored
      Rust database, a TypeScript SDK, a REST API, and an OpenAPI spec —
      specialized to your schema at compile time. Not a runtime ORM.
    `,
    ctaPrimary: dd`Get started`,
    ctaGithub: dd`GitHub`,
    install: dd`cargo install forgedb`,
  },

  showcase: {
    heading: dd`One schema in, a full stack out`,
    body: dd`
      The schema-specific surface — types, queries, filters, relations, routes —
      is generated and tailored per app. No generic engine reflects your schema
      at runtime.
    `,
    schema: dd`
      // schema.forge — the single source of truth
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
      }
    `,
    sdk: dd`
      // generated TypeScript SDK — fully typed from your schema
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
      });
    `,
  },

  invariant: {
    lead: dd`
      The invariant: your schema is a <Hl>compile-time input to generation</Hl>,
      never a runtime input to a generic engine.
    `,
    body: dd`
      Generated code links only schema-agnostic substrate crates — storage, WAL,
      change feed, auth — that know nothing about any specific schema. The
      tailored data logic stays generated. [Read the concepts →](/docs/concepts/)
    `,
  },

  features: {
    heading: dd`Batteries generated in`,
    body: dd`
      Durability, concurrency, real-time, and multi-tenancy — generated per
      schema over a small, published substrate. Each is honest about its limits.
    `,
    learnMore: dd`Learn more`,
    items: [
      {
        icon: "ShieldCheck",
        href: "/docs/concepts/",
        title: dd`Compile-time type safety`,
        body: dd`
          Your schema becomes tailored Rust and TypeScript types. Schema drift is
          impossible — the types are generated, not reflected at runtime.
        `,
      },
      {
        icon: "Columns3",
        href: "/docs/features/indexes/",
        title: dd`Columnar storage`,
        body: dd`
          Fixed-width and variable-length columns with a tiny on-disk footprint
          and column-pruned scans generated per model.
        `,
      },
      {
        icon: "HardDriveDownload",
        href: "/docs/features/durability/",
        title: dd`Crash-safe durable writes`,
        body: dd`
          Every write hits a per-model WAL with an fsync barrier before columns,
          with torn-tail recovery and a single-writer lock.
        `,
      },
      {
        icon: "GitBranch",
        href: "/docs/features/transactions-mvcc/",
        title: dd`Transactions & MVCC`,
        body: dd`
          Atomic generated transactions, optimistic concurrent writers, and a
          multi-process commit coordinator — three strict tiers.
        `,
      },
      {
        icon: "Radio",
        href: "/docs/features/live-queries/",
        title: dd`Live queries & change feed`,
        body: dd`
          Subscribe over WebSockets to typed inserts, updates, and removals —
          result-set deltas generated from the same filters as your REST API.
        `,
      },
      {
        icon: "Building2",
        href: "/docs/features/multi-tenancy/",
        title: dd`Multi-tenancy & auth`,
        body: dd`
          Physical dir-per-tenant isolation with a verify-only JWT tenant guard —
          process-per-tenant, generated code stays tenant-oblivious.
        `,
      },
      {
        icon: "Globe",
        href: "/docs/features/browser-replica/",
        title: dd`Browser read-replica`,
        body: dd`
          Compile the same generated database to WASM and follow a durable
          replication stream — query a local replica in the browser over
          IndexedDB or OPFS.
        `,
      },
      {
        icon: "History",
        href: "/docs/features/snapshot-reads/",
        title: dd`Point-in-time reads`,
        body: dd`
          Lock-free watermark snapshots with zero version machinery — read the
          database as of an earlier commit over the same REST surface.
        `,
      },
      {
        icon: "Archive",
        href: "/docs/features/migrations/",
        title: dd`Backup & migrations`,
        body: dd`
          Lock-free full-snapshot backup/restore, plus a version-guarded
          migration workflow with an offline data transformer.
        `,
      },
    ],
  },

  stats: {
    heading: dd`Small, fast, and honest`,
    body: dd`
      Benchmarked fairly — matched durability across engines. At the fsync-barrier
      tier ForgeDB ties SQLite and redb; relaxed, it's the fastest of the group.
    `,
    cta: dd`See the numbers`,
    items: [
      { value: dd`1 schema`, label: dd`→ Rust DB + TS SDK + REST API + OpenAPI` },
      { value: dd`1.88 MB`, label: dd`on-disk footprint — smallest of the embedded four` },
      { value: dd`0.37 s`, label: dd`group-commit bulk load of 10k rows` },
      { value: dd`0 deps`, label: dd`on any runtime ORM — only schema-agnostic substrate` },
    ],
  },

  steps: {
    heading: dd`From schema to server`,
    body: dd`
      The whole workflow is three commands. Everything else is generated.
    `,
    items: [
      {
        n: "01",
        title: dd`Write a schema`,
        body: dd`
          Describe your models, relations, and constraints in a declarative
          .forge file.
        `,
        code: dd`
          User {
            id: +uuid
            email: &string @email
            posts: [Post]
          }
        `,
        lang: "forge",
      },
      {
        n: "02",
        title: dd`Generate`,
        body: dd`
          Transpile the schema into tailored Rust, a TypeScript SDK, a REST API,
          and an OpenAPI spec.
        `,
        code: dd`forgedb generate all --output ./generated`,
        lang: "bash",
      },
      {
        n: "03",
        title: dd`Build & serve`,
        body: dd`
          Compile the generated crate and run the server — a real REST API over
          columnar storage.
        `,
        code: dd`forgedb build && ./target/release/server`,
        lang: "bash",
      },
    ],
  },

  cta: {
    heading: dd`Generate your database`,
    body: dd`
      Install the CLI, write a schema, and ship a typed full-stack database in
      minutes.
    `,
    primary: dd`Start the quickstart`,
    secondary: dd`Browse the docs`,
  },
} satisfies LandingCopy;
