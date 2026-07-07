# ForgeDB — Backlog

The backlog now lives in **GitHub Issues**, not this file. This page is just a map.

- Browse: https://github.com/hoodiecollin/forgedb/issues
- Add work as a new issue; close it when done (git history is the record).

## Labels

- **`tech-debt`** — known gaps or stubs in shipped code. Grounded in the codebase, ready to schedule.
- **`idea`** — speculative feature directions. Each needs a design note before implementation.
- **`enhancement`** / **`bug`** / **`documentation`** — standard GitHub labels.

## Current known gaps (`tech-debt`)

- [#47](https://github.com/hoodiecollin/forgedb/issues/47) — `@computed` expression convention + wire `validate --implementations` (string-literal lexing from #46 landed; still needs an expression grammar)
- [#48](https://github.com/hoodiecollin/forgedb/issues/48) — query-optimization: join predicate pushdown (needs a predicate IR)
- [#49](https://github.com/hoodiecollin/forgedb/issues/49) — Restore OpenAPI generation

## Feature ideas (`idea`)

Each needs a design note first. The **next stack** is labeled
[`plan-next`](https://github.com/hoodiecollin/forgedb/issues?q=is%3Aissue+is%3Aopen+label%3Aplan-next)
(#50–52, #56, #57, #59, #62, #63); everything else is eligible for later priority. Grouped by theme:

- **Runtimes & distribution** — WASM ([#50](https://github.com/hoodiecollin/forgedb/issues/50)),
  Python bindings ([#51](https://github.com/hoodiecollin/forgedb/issues/51)),
  Node.js addon ([#52](https://github.com/hoodiecollin/forgedb/issues/52)),
  Deno bindings ([#53](https://github.com/hoodiecollin/forgedb/issues/53))
- **Storage & data** — replication ([#54](https://github.com/hoodiecollin/forgedb/issues/54)),
  zero-knowledge storage ([#55](https://github.com/hoodiecollin/forgedb/issues/55)),
  MVCC ([#56](https://github.com/hoodiecollin/forgedb/issues/56)),
  backup & restore ([#57](https://github.com/hoodiecollin/forgedb/issues/57)),
  time-series ([#58](https://github.com/hoodiecollin/forgedb/issues/58)),
  multi-tenancy ([#59](https://github.com/hoodiecollin/forgedb/issues/59))
- **Indexing & query** — advanced indexes ([#60](https://github.com/hoodiecollin/forgedb/issues/60)),
  GraphQL ([#61](https://github.com/hoodiecollin/forgedb/issues/61)),
  real-time subscriptions ([#62](https://github.com/hoodiecollin/forgedb/issues/62))
- **Tooling & DX** — database inspector ([#63](https://github.com/hoodiecollin/forgedb/issues/63)),
  AI-powered development ([#64](https://github.com/hoodiecollin/forgedb/issues/64))

## Notes

- Favor depth on existing features over breadth. Not all ideas will be built.
- Prefer *better generated code* over *runtime functionality* (guard the generator identity —
  see the `forgedb-product-manager` subagent).
