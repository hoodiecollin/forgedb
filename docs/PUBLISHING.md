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
  `forgedb-backup`, `forgedb-watcher`, `forgedb-lsp-server`, and the root `forgedb`
  CLI binary.
  - `forgedb-lsp-server` joined this list in epic #173: the `forgedb` crate now
    has an **optional** dependency on it (the non-default `lsp` feature drives the
    bundled `forgedb-lsp` binary). crates.io requires every dependency — optional
    included — to be resolvable, so **publish `forgedb-lsp-server` before the next
    `forgedb` publish** (and it must be on the registry for
    `cargo install forgedb --features lsp` to build). Same publish-gap rule as the
    substrate crates.

**Not published:**

- `apps/inspector/src-tauri` (`publish = false`).

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
forgedb-lsp-server          # → parser, validation
forgedb-migrations
forgedb-backup              # → storage

# CLI last
forgedb                     # → parser, codegen, watcher, migrations, compaction, backup, coordinator
```

In practice you only publish the crates that changed this cycle, plus any whose `version =`
pin you bumped as a consequence.

### Do not decide "what changed" by comparing version numbers

The dangerous state is **an in-tree crate whose version equals the published version but whose
source does not**. Nothing local detects it: the workspace builds fine, because path deps shadow
the registry. It only surfaces for a user, as generated code that will not compile against
crates.io.

Comparing `grep '^version' crates/*/Cargo.toml` to `cargo search` cannot see this — the numbers
match, which is precisely the bug. Diff against the published artifact instead:

```bash
# for each crate, fetch what is actually on crates.io and diff the source
curl -sL "https://static.crates.io/crates/$C/$C-$V.crate" | tar xz
diff -rq "$C-$V/src" "crates/<dir>/src"
```

Every crate that reports a difference needs a publish. (Reconciling the v0.4.0 gap this way found
six drifted crates where a version-number comparison found one.)

### …but a clean `src/` diff does NOT mean "no publish"

The converse of the rule above is false, and it fails silently. A crate whose only change is a
**dependency version requirement** has a byte-identical `src/`, so the diff reports it clean while
it still must publish — its *manifest* is what changed, and cargo rewrites `Cargo.toml` at publish
time, which is why you cannot simply diff that too.

Two crates in this workspace are permanently this shape:

- **`forgedb-storage`** — a pure `cfg` re-export facade. Its entire content is two backend pins, so
  a backend bump changes the crate completely while touching no source line.
- **`forgedb-watcher`** — pins `forgedb-codegen`, and drives the generators directly.

The watcher case shows why "it still compiles" is not reassurance. Leave its pin behind and a
registry resolution hands the root CLI `forgedb-codegen 0.4.0` while handing watcher
`forgedb-codegen 0.3.0` — two semver-incompatible copies. It **builds**, because no codegen type
crosses watcher's public boundary, and `forgedb dev`'s watch path then quietly regenerates with the
previous cycle's generators, rejecting syntax the non-watch path accepts.

So run a **second, independent check** beside the source diff: sweep every intra-workspace
`forgedb-*` requirement against the depended-on crate's in-tree version and require that the caret
range admits it.

```bash
# every intra-workspace pin, next to the version it must admit
grep -Hn 'forgedb-[a-z-]* *= *{.*version' Cargo.toml crates/*/Cargo.toml
grep -H  '^version' crates/*/Cargo.toml
```

A stale pin is invisible to everything local: the workspace build passes (path deps shadow the
registry), and so does `cargo publish --dry-run`. It fails only for someone resolving from
crates.io — every installed user, and nobody in this repo.

**The rule to apply: when you republish a crate, tighten its dependents' pins to the new version
and republish those dependents too.** A dependent still pinning the old version is the same
stale-source hazard, one level out.

### Check for a breaking change before choosing the bump

A crate that is only *additive* takes a patch/minor bump. A crate that removed or changed a
public item needs the pre-1.0 **minor** position bumped (`0.2.x → 0.3.0`), and getting this wrong
breaks the *already-released* CLI rather than anything you are about to ship:

> Publishing a renamed enum variant as `forgedb-parser 0.2.2` would have been resolved by the
> caret requirements inside the published `forgedb 0.3.1`, `forgedb-codegen 0.2.2` and
> `forgedb-lsp-server 0.1.0` — all of which still referenced the old name. `cargo install forgedb`
> would have started failing for every user, from a publish that touched none of their crates.

Two questions decide it:

1. **Did any public item change or disappear?** Diff the published `src/` (above) and look for
   removed/renamed `pub` items, new public struct fields (breaks literal construction), or new
   enum variants on a non-`#[non_exhaustive]` enum.
2. **Does a dependent leak the changed type in *its* public API?** If so the major propagates.
   `forgedb-codegen` takes `&forgedb_parser::Schema` in `generate()`, so a parser major forces a
   codegen major; `forgedb-watcher` and `forgedb-lsp-server` expose no parser type, so they took
   patch bumps in the same cascade.

Before publishing, confirm the blast radius by grepping the *published* sources of every
downstream crate for the item you changed — not the working tree, which has already moved on.

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

Generated code pins substrate crates by version range in the **generated cache manifests**. If
generated code starts requiring a **new substrate crate** or a **new additive substrate API**,
that crate/version must be **published before** the generator pins it — otherwise an
outside-repo `init → generate → build` cannot resolve from crates.io.

The reclose is **proven**, not assumed: after publishing, run an isolated

```bash
# in a throwaway dir OUTSIDE this repo, with FORGEDB_HOME also outside it
export FORGEDB_HOME="$(mktemp -d)"
forgedb init app
forgedb generate all --schema app/schema.forge
forgedb build        --schema app/schema.forge   # every forgedb-* dep from crates.io, 0 errors
```

**`forgedb build`, not `cargo build`** — since #335 there is no crate in the scaffold to build,
and the command under test is the one an installed user actually runs. `FORGEDB_HOME` must be
outside the checkout, and that is an assertion rather than a nicety: this repo's root
`Cargo.toml` has `members` and no `exclude`, so a cache created inside the working tree joins
*this workspace* and inherits its pinned toolchain and path dependencies — the reclose would be
measuring the checkout instead of the registry.

Then confirm the resolution came from the registry, in the **cache** lockfile
(`$FORGEDB_HOME/projects/<id>/Cargo.lock`) — there is no lockfile under the app any more:

```bash
grep -c 'registry+https://github.com/rust-lang/crates.io-index' "$FORGEDB_HOME"/projects/*/Cargo.lock
```

State it positively — *every* `forgedb-*` package carries a registry source, and at least N of
them do — never as "no `path+…` source appears". On cargo 1.96 / lockfile v4 a path dependency
records **no `source` key at all**, so the negative form matches nothing whether or not the bug
is present: it is a check that passes having inspected nothing.

A `cargo generate-lockfile` against the cache root is the cheap half of this: it exits non-zero
on an unpublished version, with the candidate list in the message, and compiles nothing. It
catches the *missing-version* half of a publish gap; only the compiles catch the *stale-source*
half.

For the **browser replica**, the reclose is `forgedb generate browser --replica` + `forgedb
build`, which resolves `forgedb-storage-web` from the registry and compiles against it. ForgeDB
no longer runs `rustup target add` for you — install `wasm32-unknown-unknown` yourself, and note
the honest limit if the wasm toolchain can't run in a given environment.

Track the open/closed state of a gap in the project's GitHub issues (a knowingly deferred one is
a `release-gate`), and update the generated pins when you publish.

---

## Version pins to update alongside a publish

When you bump a substrate crate that generated code links, also update where the CLI emits its
pin. **The `init` scaffold no longer carries any** — it scaffolds no cargo package — so the pin
lists live in codegen:

- `crates/codegen/src/core_pkg.rs` — the app's database crate, and the **largest** pin list
  (storage, types, changefeed, wal, compaction, txn, the host-only coordinator, plus `utoipa`
  when the app has a server),
- `crates/codegen/src/server_pkg.rs` — the API binary (`forgedb-auth` with `jwks-http`,
  `forgedb-query-params`, `forgedb-changefeed`, and the axum/utoipa stack),
- `crates/codegen/src/transform.rs` — the offline migration bins, which pin substrate
  **directly** because they cannot link `core` (a hop is pinned to its version range, not to
  whatever the current schema is),
- the substrate version matrix in [INSTALL.md](./INSTALL.md),
- the substrate crate table in [PUBLIC_CRATES.md](./PUBLIC_CRATES.md).

The four wrapper packages — `napi/`, `pyo3/`, `ffi/`, `wasm/` — pin **zero** substrate crates
and reach it through `core`, so there is nothing to bump there and a `grep '^forgedb-'` on one
of their manifests correctly matches nothing.

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
- [`CLAUDE.md`](../CLAUDE.md) — authoritative workspace inventory
- [Cargo Book — Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
