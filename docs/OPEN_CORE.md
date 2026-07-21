# Open-Core Boundary

This document states, precisely and durably, **what ForgeDB gives away and what it does not** —
so contributors, users, and future-us all read the boundary the same way. It is the companion to
[`SEMVER.md`](./SEMVER.md) (stability guarantees) and [`PUBLIC_CRATES.md`](./PUBLIC_CRATES.md)
(the substrate surface).

## TL;DR

**Everything in this repository is open source under `MIT OR Apache-2.0`.** The generator, the
CLI, the substrate crates, the docs, the examples, the benchmarks, the desktop Inspector, and the
editor extension are all free to use, fork, embed, and ship — including commercially.

ForgeDB is **not** monetized by closing the core. It is monetized (if and when it is) by a
**hosted service** built on top of the open core, plus any future add-ons authored as *new,
separately-licensed* components. Nothing currently in this repo is or will become closed.

## Why this boundary, and why it can't move backward

The core generator — `parser`, `codegen`, `validation`, `migrations` and the schema-agnostic
substrate crates — is **already published to crates.io under `MIT OR Apache-2.0`**. Permissive
open source is not reversible: the versions already published are forkable forever. Rather than
pretend otherwise, ForgeDB embraces it. A widely-adopted, permissively-licensed generator is the
**top of the funnel**; the commercial surface sits *above* it, never *inside* it.

This means the project will **never**:

- relicense a published crate to a restrictive license and call the old one deprecated,
- move tailored, generated logic into a closed component to create artificial lock-in, or
- gate a core capability (generate → compile → run your own database) behind a paywall.

## What is open (this repository)

| Component | Path | Notes |
|---|---|---|
| **`forgedb` CLI** | `src/` | `init`, `generate`, `build`, `dev`, `migrate`, `backup`, `tenant`, `coordinate`, … |
| **Compiler internals** | `crates/{parser,codegen,validation,migrations,backup,watcher}` | Published so `cargo install forgedb` builds from the registry. Per `SEMVER.md`, **not** a stable public API. |
| **Substrate crates** | `crates/{types,storage,storage-native,storage-web,wal,changefeed,auth,query-params,compaction,txn,coordinator}` | Schema-agnostic. Stable public API (semver'd). Generated code links these. |
| **LSP server** | `crates/lsp-server` | Editor language features. |
| **Docs / examples / benchmarks** | `docs/`, `examples/`, `benchmarks/` | The adoption surface — kept honest on purpose. |
| **Inspector** | `apps/inspector` | A **generic dev/ops tool** (Next.js + Tauri) that talks to any running ForgeDB server over its REST/WS contract. It is *not* a generated per-schema artifact, and it is *not* the shipped app's data plane — it is developer tooling, like `psql`. Free and open. |
| **Website** | `apps/website` | Marketing + docs front door. |
| **VS Code extension** | `vscode-forgedb` | Free. |

## What a commercial offering would add (built separately, not by closing the above)

None of the following exists in this repository. If ForgeDB is monetized, this is the surface —
**net-new components on top of the open core**, not a subtraction from it:

- **Hosted / managed service:** provisioning, a managed multi-process write coordinator, hosted
  backups and point-in-time restore, dashboards, observability, and billing.
- **Operational add-ons:** managed replication endpoints, fleet management across tenants,
  SLA-backed support.
- **Future proprietary generator plugins**, *if any*, authored as **new crates never published to
  crates.io** under a commercial or source-available license. The identity invariant makes this
  clean: because every app's data logic is *generated per-schema*, a premium generation target is a
  self-contained unit that composes with — rather than hollows out — the open core.

## The principle that governs the boundary

The line is the same one that governs the whole project's architecture (see `CLAUDE.md`, *"What
ForgeDB is"*):

> The app's schema is a **compile-time input to generation**, never a **runtime input to a generic
> engine**. Every published artifact is either schema-agnostic substrate or transport glue — never a
> generic runtime that interprets a user's schema.

The commercial boundary rides on top of that architectural one:

- **Open** = anything that helps you *generate, build, and run your own ForgeDB database* — the
  compiler, the substrate, the tooling.
- **Commercial** = the *operational layer that runs it for you at scale* — provisioning, managed
  coordination, hosted durability/replication, support — none of which reads your schema or reduces
  what the open core can do on its own.

If a proposed "paid feature" would require closing something already open, or would make the free
generator produce a worse database so the paid tier looks better, it is the wrong feature. Redesign
it as an additive operational layer or drop it.
