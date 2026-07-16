# ForgeDB Publishing Guide

**Audience:** maintainers publishing `forgedb-*` crates to crates.io.

This is the operational runbook. The compatibility *policy* it enforces (what a version bump
means on each surface) lives in [SEMVER.md](./SEMVER.md); the crate catalog is
[PUBLIC_CRATES.md](./PUBLIC_CRATES.md).

---

## What gets published, and why

Three groups, each treated differently:

**Substrate crates** — a stability surface, because generated code links them. Published on
**independent version lines that are intentionally NOT normalized**:

- `forgedb-types`, `forgedb-storage` (facade) + `forgedb-storage-native` + `forgedb-storage-web`,
  `forgedb-wal`, `forgedb-changefeed`, `forgedb-auth`, `forgedb-query-params`,
  `forgedb-compaction`, `forgedb-txn`, `forgedb-coordinator`.

**Compiler internals** — published to crates.io **only so `cargo install forgedb` can build the
CLI from the registry**. NOT a stable API ([SEMVER.md §4](./SEMVER.md)):

- `forgedb-parser`, `forgedb-codegen`, `forgedb-validation`, `forgedb-migrations`,
  `forgedb-backup`, `forgedb-watcher`, and the root `forgedb` CLI binary.

**Not published:**

- `forgedb-lsp-server`; `apps/inspector/src-tauri` (`publish = false`).

> There is **no single coordinated version**. Do not run a script that rewrites every crate to
> one version — that contradicts the substrate design. Bump only the crate(s) whose public API,
> behavior, or on-disk/on-wire format actually changed, per [SEMVER.md](./SEMVER.md).

---

## When to bump (and by how much)

Follow [SEMVER.md](./SEMVER.md). In short, pre-1.0:

- **Breaking** (API change, or a change to on-disk/on-wire format a prior binary can't read) →
  bump the crate's **minor** (`0.1.x → 0.2.0`).
- **Additive** (new method, new optional behavior, backward-compatible) → bump the crate's
  **patch** (`0.2.1 → 0.2.2`), or minor if it's a substantial additive surface.

An additive method on an existing substrate crate still requires a publish before generated code
may rely on it — see "The publish gap" below.

---

## Dependency & publish order

Publish leaves first; a crate must be on crates.io before anything that path-depends on it with
a `version =` requirement is published.

```
# leaves (no forgedb deps) — any order
forgedb-types
forgedb-wal
forgedb-changefeed
forgedb-auth
forgedb-query-params
forgedb-compaction
forgedb-txn

# storage stack (wal must be up first)
forgedb-storage-native      # → wal
forgedb-storage-web         # → wal
forgedb-storage             # → storage-native / storage-web (cfg-selected)

# coordinator (txn + changefeed must be up first)
forgedb-coordinator         # → txn, changefeed

# compiler internals (for `cargo install` only)
forgedb-validation
forgedb-parser              # → validation
forgedb-codegen             # → parser
forgedb-watcher             # → codegen, parser
forgedb-migrations
forgedb-backup              # → storage

# CLI last
forgedb                     # → parser, codegen, watcher, migrations, compaction, backup, coordinator
```

In practice you only publish the crates that changed this cycle, plus any whose `version =`
pin you bumped as a consequence.

---

## Publishing steps (per crate)

Run everything from the repo root; use `-p <crate>` rather than `cd`.

```bash
# 1. Verify contents
cargo package -p forgedb-<crate> --list

# 2. Dry run (resolves deps from the registry as they'd be seen post-publish)
cargo publish -p forgedb-<crate> --dry-run

# 3. Publish
cargo publish -p forgedb-<crate>

# 4. Let the index settle before publishing a dependent
#    (a few seconds; the dry-run of the next crate will fail if it's not indexed yet)
```

`cargo` reads `[workspace.package]` for edition/etc., so per-crate manifests inherit correctly.
Each substrate crate carries its own `description`/`keywords`/`categories`.

---

## The publish gap (the load-bearing discipline)

Generated code pins substrate crates by caret range in the scaffold `Cargo.toml`. If generated
code starts requiring a **new substrate crate** or a **new additive substrate API**, that
crate/version must be **published before** `forgedb init` pins it — otherwise an outside-repo
`init → generate → cargo build` cannot resolve from crates.io.

The reclose is **proven**, not assumed: after publishing, run an isolated

```bash
forgedb init --template blog        # in a throwaway dir outside this repo
forgedb generate rust && forgedb generate api
cargo build                          # must resolve every forgedb-* dep from crates.io, 0 errors
```

and confirm the generated `Cargo.lock` resolved the freshly-published versions from
`registry+https://.../crates.io-index`. `CLAUDE.md` tracks the current open/closed state of this
gap; keep it updated when you publish.

For the **wasm** replica, the equivalent reclose is `wasm-pack build --target web` against
`forgedb-storage-web` + the wasm-flavored `types`/`wal`; note its own honest limits in
`CLAUDE.md` if the wasm toolchain can't run in a given environment.

---

## Version pins to update alongside a publish

When you bump a substrate crate that generated code links, also update where the CLI emits its
pin:

- the scaffold `Cargo.toml` template in `src/commands/init.rs` (the caret pins),
- the substrate version matrix in [INSTALL.md](./INSTALL.md),
- the relevant status note in [`CLAUDE.md`](../CLAUDE.md).

Include `Cargo.lock` changes in the same commit when they're a side effect.

---

## Distribution: `cargo install` and prebuilt binaries

- `cargo install forgedb` builds the CLI from crates.io — which is why the six compiler-internal
  crates above are published. Prove it with an isolated `CARGO_HOME` install resolving the whole
  closure from the registry.
- Prebuilt cross-platform binaries are produced by `.github/workflows/release.yml` on a `v*` tag
  push (Linux x86_64/aarch64, macOS Intel/ARM, Windows → a GitHub Release). See
  [INSTALL.md](./INSTALL.md) for every install path.

---

## Yanking

If a published version is broken:

```bash
cargo yank -p forgedb-<crate> --version X.Y.Z          # block new resolutions
cargo yank -p forgedb-<crate> --version X.Y.Z --undo   # reverse it
```

Yanking does not delete; it prevents *new* dependency resolutions from selecting the version.
Follow with a fixed patch release and update the pins as above.

---

## References

- [SEMVER.md](./SEMVER.md) — the compatibility policy this runbook enforces
- [PUBLIC_CRATES.md](./PUBLIC_CRATES.md) — substrate crate catalog + dependency graph
- [INSTALL.md](./INSTALL.md) — install paths + substrate version matrix
- [`CLAUDE.md`](../CLAUDE.md) — authoritative inventory + live publish-gap status
- [Cargo Book — Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
