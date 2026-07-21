# @forgedb/website

The public marketing + documentation site for ForgeDB. A static Next.js app —
marketing landing, a full `/docs` section (schema language, CLI, features, config,
reference), and an examples gallery.

## Stack

Mirrors `apps/inspector`: Next.js 16 (app router), React 19, TypeScript (strict),
Tailwind v4 (CSS-first), shadcn/ui (`radix-nova`), jotai + next-themes, bun. Docs are
MDX compiled at build time with `next-mdx-remote/rsc`; code is highlighted by Shiki,
including authentic `.forge` highlighting from the VS Code extension's own TextMate
grammar (`vscode-forgedb/syntaxes/forge.tmLanguage.json`). Search is a client-side ⌘K
palette over a prebuilt static index.

## Develop (from the repo root — no `cd` needed)

```bash
make website-install     # install JS deps
make website             # dev server on http://localhost:3100
make website-typecheck   # tsc --noEmit
make website-build       # static export → apps/website/out
```

## Content

- **Docs** live as MDX under `content/docs/**`. The sidebar / prev-next / search index
  are driven by `lib/docs-nav.ts` — add a page there and create the matching
  `content/docs/<slug>.mdx`. The `prebuild` step (`scripts/build-search-index.ts`)
  regenerates the search index and fails the build on any dead nav link.
- **Examples** are read at build time from the repo's `examples/*/schema.forge`; catalog
  metadata lives in `lib/examples.ts`.

## Deploy

`output: "export"` produces a fully static `out/` directory — host it anywhere
(Cloudflare Pages, GitHub Pages, Vercel, an object store). No Node server at runtime.
