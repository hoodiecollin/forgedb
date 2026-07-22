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

## 2. Package managers & prebuilt binaries (no Rust toolchain needed)

> **Activates with the first tagged release.** These channels all repackage the
> per-platform binaries attached to a
> [GitHub Release](https://github.com/hoodiecollin/forgedb/releases). The release
> pipeline (cargo-dist + a maturin PyPI sidecar) is wired and validated, but no
> `v*` tag has been pushed yet — until then, use `cargo install` (above) or
> `--git` (below). The commands below are what each channel will be once a
> release is cut; every channel ships the same `forgedb` binary (plus the bundled
> `forgedb-lsp` language server) and stays version-locked to the crate release.

**Shell one-liner (macOS / Linux):**

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/hoodiecollin/forgedb/releases/latest/download/forgedb-installer.sh | sh
```

**PowerShell (Windows):**

```powershell
powershell -c "irm https://github.com/hoodiecollin/forgedb/releases/latest/download/forgedb-installer.ps1 | iex"
```

**Homebrew (macOS / Linux):**

```bash
brew install hoodiecollin/tap/forgedb
```

**npm** (published under a personal scope — the unscoped name was unavailable):

```bash
npm install -g @hoodiecollin/forgedb    # or: npx @hoodiecollin/forgedb --help
```

**uv / pip (PyPI):**

```bash
uv tool install forgedb    # or: uvx forgedb --help
pip install forgedb
```

**Direct download.** Each release also attaches raw archives — grab the one for
your platform, extract, and put `forgedb` on your `PATH`:

| Platform | Asset |
|---|---|
| Linux x86_64 (glibc) | `forgedb-x86_64-unknown-linux-gnu.tar.xz` |
| Linux x86_64 (musl) | `forgedb-x86_64-unknown-linux-musl.tar.xz` |
| Linux aarch64 | `forgedb-aarch64-unknown-linux-gnu.tar.xz` |
| macOS (Intel) | `forgedb-x86_64-apple-darwin.tar.xz` |
| macOS (Apple Silicon) | `forgedb-aarch64-apple-darwin.tar.xz` |
| Windows x86_64 | `forgedb-x86_64-pc-windows-msvc.zip` |

```bash
tar -xf forgedb-<target>.tar.xz
sudo mv forgedb forgedb-lsp /usr/local/bin/     # or anywhere on your PATH
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
| `forgedb-storage` | `0.2` | Columnar storage facade (native positional-I/O / wasm arena) |
| `forgedb-wal` | `0.2` | Write-ahead log (opaque `Raw` durable-write path) |
| `forgedb-changefeed` | `0.2` | Change-feed broadcast + durable resumable broker substrate |
| `forgedb-auth` | `0.1` | Verify-only JWT + tenant cross-check middleware |
| `forgedb-query-params` | `0.1` | REST query-string → generic filter/sort/paginate |
| `forgedb-compaction` | `0.1` | In-process dead-row reclaim (keep-set GC) |
| `forgedb-txn` | `0.1` | MVCC Tier-2 commit sequencer (monotonic LSN, conflict detect) |
| `forgedb-coordinator` | `0.2` | MVCC Tier-3 multi-process write coordinator |

These are **class-1 substrate**: they know nothing about any specific schema. All
schema-tailored logic (types, tables, queries, filters, relations, API routes)
is *generated* per app at compile time — never reconstructed at runtime by a
generic engine. See `CLAUDE.md` / `docs/ARCHITECTURE.md` for the identity
invariant, and `docs/SEMVER.md` for the compatibility policy behind these
version lines.

> Version pins are intentionally **not** normalized across the substrate — each
> crate has an independent version line and is bumped only when it changes.
