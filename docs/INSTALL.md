# Installing ForgeDB

ForgeDB ships as a single CLI binary, `forgedb`. Pick whichever install method
fits — they all give you the same tool.

## 1. `cargo install` (from crates.io)

The canonical path if you have a Rust toolchain (≥ 1.85, edition 2024):

```bash
cargo install forgedb
```

This builds the CLI from the published crates and drops `forgedb` in
`~/.cargo/bin`. Update later with `cargo install forgedb --force`.

## 2. Prebuilt binaries (no toolchain needed)

Each tagged release attaches prebuilt binaries to its
[GitHub Release](https://github.com/hoodiecollin/forgedb/releases). Download the
archive for your platform, extract, and put `forgedb` on your `PATH`:

| Platform | Asset |
|---|---|
| Linux x86_64 | `forgedb-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux aarch64 | `forgedb-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS (Intel) | `forgedb-<version>-x86_64-apple-darwin.tar.gz` |
| macOS (Apple Silicon) | `forgedb-<version>-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `forgedb-<version>-x86_64-pc-windows-msvc.zip` |

```bash
tar -xzf forgedb-<version>-<target>.tar.gz
sudo mv forgedb /usr/local/bin/     # or anywhere on your PATH
forgedb --help
```

## 3. `cargo install --git` (latest from source)

To track the tip of `main` (or before a release is cut):

```bash
cargo install --git https://github.com/hoodiecollin/forgedb forgedb
```

This builds the whole workspace from source, so no crates need to be published
first.

## 4. Build from a clone

```bash
git clone https://github.com/hoodiecollin/forgedb
cd forgedb
cargo build --release          # binary at target/release/forgedb
cargo install --path .         # or install it onto your PATH
```

---

## Substrate crate versions

A ForgeDB-generated app is **not** dependency-free: its generated Rust code links
a set of small, **schema-agnostic** runtime crates published on crates.io (the
"substrate"). `forgedb init` pins these in the generated `Cargo.toml`, so you
normally don't manage them by hand — this table is the reference for what a given
CLI generates against.

| Crate | Version | Role |
|---|---|---|
| `forgedb-types` | `0.2` | Core type system (uuid, timestamp, primitives) |
| `forgedb-storage` | `0.1.5` | Columnar storage engine (positional-I/O columns) |
| `forgedb-wal` | `0.2` | Write-ahead log (opaque `Raw` durable-write path) |
| `forgedb-changefeed` | `0.1` | In-process change-feed broadcast substrate |
| `forgedb-auth` | `0.1` | Verify-only JWT + tenant cross-check middleware |
| `forgedb-query-params` | `0.1` | REST query-string → generic filter/sort/paginate |
| `forgedb-compaction` | `0.1` | In-process dead-row reclaim (keep-set GC) |

These are **class-1 substrate**: they know nothing about any specific schema. All
schema-tailored logic (types, tables, queries, filters, relations, API routes)
is *generated* per app at compile time — never reconstructed at runtime by a
generic engine. See `CLAUDE.md` / `docs/ARCHITECTURE.md` for the identity
invariant, and `docs/SEMVER.md` for the compatibility policy behind these
version lines.

> Version pins are intentionally **not** normalized across the substrate — each
> crate has an independent version line and is bumped only when it changes.
