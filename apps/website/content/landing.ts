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

export interface ClientTab {
  /** stable tab id (radix value) */
  id: string;
  /** tab label (plain text) */
  label: string;
  /** shiki language + CodeBlock filename */
  lang: string;
  filename: string;
  /** code sample (plain source, not markdown) */
  code: string;
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
  };
  clients: {
    heading: string;
    body: string;
    note: string;
    tabs: ClientTab[];
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
    heading: dd`Your schema *is* the <Hl>database</Hl>`,
    subhead: dd`
      Write one <code>.forge</code> schema. ForgeDB compiles it into a real
      columnar database, a REST API, and typed clients for
      <Hl>TypeScript, Python, Rust, and Go</Hl> — every type, query, and route
      tailored to your schema at compile time. Not a runtime ORM. Nothing reflects
      your schema at runtime.
    `,
    ctaPrimary: dd`Get started`,
    ctaGithub: dd`GitHub`,
    install: dd`cargo install forgedb`,
  },

  showcase: {
    heading: dd`Write the schema. Get the stack.`,
    body: dd`
      Every type, query, filter, relation, and route is generated and tailored to
      *your* models — then compiled. The database that runs already knows your
      data by name; nothing reflects a schema at runtime.
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
  },

  clients: {
    heading: dd`Typed clients in the languages you already ship`,
    body: dd`
      The same schema generates a network client for every language on your stack
      — dataclasses in Python, structs in Rust and Go, interfaces in TypeScript.
      Same methods, same shapes, no hand-written HTTP and no drift between them.
    `,
    note: dd`
      Prefer to embed the database instead of calling it over HTTP? The same
      schema also generates <Hl>in-process native bindings</Hl> — PyO3 for Python,
      NAPI-RS for Node and Bun, and a WASM read-replica for the browser.
      [See the generate command →](/docs/cli/generate/)
    `,
    tabs: [
      {
        id: "ts",
        label: "TypeScript",
        lang: "typescript",
        filename: "app.ts",
        code: dd`
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
      {
        id: "py",
        label: "Python",
        lang: "python",
        filename: "app.py",
        code: dd`
          from forgedb_client import ForgeDbClient, PostCreate, ListOptions

          db = ForgeDbClient("http://localhost:3000")

          # create_post() takes a typed PostCreate; returns the new id
          post_id = db.create_post(PostCreate(
              title="Hello, ForgeDB",
              slug="hello-forgedb",
              views=0,
              published=True,
              author=user_id,
          ))

          # list_post() → ListResult { data, total, limit, offset }
          page = db.list_post(ListOptions(
              filter={"published": "true"},
              sort="views",
              order="desc",
              limit=10,
          ))
        `,
      },
      {
        id: "rs",
        label: "Rust",
        lang: "rust",
        filename: "main.rs",
        code: dd`
          use forgedb_client::{ForgeDbClient, PostCreate, ListOptions, SortOrder};

          let db = ForgeDbClient::new("http://localhost:3000");

          // create_post() takes a typed &PostCreate; returns the new id
          let id = db.create_post(&PostCreate {
              title: "Hello, ForgeDB".into(),
              slug: "hello-forgedb".into(),
              views: 0,
              published: true,
              author: user_id,
          }).await?;

          // list_post() → ListResult<Post> { data, total, limit, offset }
          let page = db.list_post(&ListOptions {
              filter: vec![("published".into(), "true".into())],
              sort: Some("views".into()),
              order: Some(SortOrder::Desc),
              limit: Some(10),
              ..Default::default()
          }).await?;
        `,
      },
      {
        id: "go",
        label: "Go",
        lang: "go",
        filename: "main.go",
        code: dd`
          import client "forgedb-client"

          db := client.NewClient("http://localhost:3000")

          // CreatePost takes a typed *PostCreate; returns the new id
          id, err := db.CreatePost(&client.PostCreate{
              Title:     "Hello, ForgeDB",
              Slug:      "hello-forgedb",
              Views:     0,
              Published: true,
              Author:    userID,
          })

          // ListPost → *ListResult[Post] { Data, Total, Limit, Offset }
          limit := 10
          page, err := db.ListPost(&client.ListOptions{
              Filter: map[string]string{"published": "true"},
              Sort:   "views",
              Order:  "desc",
              Limit:  &limit,
          })
        `,
      },
    ],
  },

  invariant: {
    lead: dd`
      Your schema is a <Hl>compile-time input to generation</Hl> — never a runtime
      input to a generic engine.
    `,
    body: dd`
      That one decision is why there's no query planner to fight, no ORM to
      out-clever, and no schema drift to chase down. The generated code already
      knows your data; the substrate it links — storage, WAL, change feed, auth —
      knows nothing about it, and never will. [Read the concepts →](/docs/concepts/)
    `,
  },

  features: {
    heading: dd`A real database, batteries generated in`,
    body: dd`
      Durability, concurrency, real-time, multi-tenancy — the things you'd bolt on
      later are generated per schema over a small, published substrate. And every
      one is honest about exactly where its limits are.
    `,
    learnMore: dd`Learn more`,
    items: [
      {
        icon: "ShieldCheck",
        href: "/docs/concepts/",
        title: dd`End-to-end type safety`,
        body: dd`
          Your schema becomes tailored types in Rust, TypeScript, Python, and Go.
          Schema drift is impossible — the types are generated at compile time,
          never reflected at runtime.
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
      { value: dd`4 languages`, label: dd`typed REST clients from one schema — TypeScript, Python, Rust, Go` },
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
          Transpile the schema into a tailored Rust database, a REST API, an
          OpenAPI spec, and typed clients for every language on your stack.
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
    heading: dd`Stop writing the same data layer`,
    body: dd`
      Install the CLI, write one schema, and let ForgeDB generate the database, the
      API, and the typed clients — so you build features, not plumbing.
    `,
    primary: dd`Start the quickstart`,
    secondary: dd`Browse the docs`,
  },
} satisfies LandingCopy;
