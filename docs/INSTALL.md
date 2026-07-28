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

> **Live as of v0.2.0.** These channels all repackage the per-platform binaries
> attached to each
> [GitHub Release](https://github.com/hoodiecollin/forgedb/releases) (driven by
> cargo-dist + a maturin PyPI sidecar). Every channel ships the same `forgedb`
> binary (plus the bundled `forgedb-lsp` language server) and stays version-locked
> to the crate release. macOS + Linux only — Windows isn't supported yet (see the
> note below).

**Shell one-liner (macOS / Linux):**

```bash
curl -fsSL https://get.forgedb.dev/install.sh | sh
```

`get.forgedb.dev/install.sh` is an alias (307) for the installer attached to the
latest GitHub Release. The direct form (identical bytes) is:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/hoodiecollin/forgedb/releases/latest/download/forgedb-installer.sh | sh
```

> **Windows is not supported yet.** v0.2.0 ships macOS + Linux binaries only —
> the `forgedb` binary is currently Unix-only (positional-I/O storage + a
> Unix-socket write coordinator), so even `cargo install` fails on native
> Windows. Windows support (native binary, the PowerShell installer, MSI, and
> winget) is tracked for a follow-up release. In the meantime, run ForgeDB under
> **WSL2**, where it behaves as Linux.

**Homebrew (macOS / Linux):**

```bash
brew install hoodiecollin/tap/forgedb
```

**npm** (published under a personal scope — the unscoped name was unavailable):

```bash
npm install -g @hoodiecollin/forgedb    # or: npx @hoodiecollin/forgedb --help
```

**uv / pip (PyPI):** the PyPI project is `hoodiecollin-forgedb` (bare `forgedb` is
taken); the installed command is still `forgedb`.

```bash
uv tool install hoodiecollin-forgedb        # command on PATH: forgedb
pip install hoodiecollin-forgedb
uvx --from hoodiecollin-forgedb forgedb --help
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

```bash
tar -xf forgedb-<target>.tar.xz
sudo mv forgedb forgedb-lsp /usr/local/bin/     # or anywhere on your PATH
forgedb --help
```

## 3. Docker

A multi-arch image (`linux/amd64` + `linux/arm64`) bundling `forgedb` and
`forgedb-lsp`, published on every release. Bind-mount your schema/output dir at
`/work`:

```bash
docker run --rm -v "$PWD:/work" ghcr.io/hoodiecollin/forgedb generate all --output ./generated
# also on Docker Hub:  docker run --rm -v "$PWD:/work" hoodiecollin/forgedb --help
```

The image repackages the same release binaries as the native install (no runtime
Rust toolchain).

## 4. Nix (flake)

Run without installing, or add ForgeDB to your own flake. Builds from source over
the committed `Cargo.lock`:

```bash
nix run    github:hoodiecollin/forgedb -- --help     # run once
nix profile install github:hoodiecollin/forgedb      # install to your profile
nix develop github:hoodiecollin/forgedb              # dev shell (rust + tooling)
```

In a flake, add `forgedb.url = "github:hoodiecollin/forgedb";` and reference
`forgedb.packages.${system}.default`.

## 5. `cargo install --git` (latest from source)

To track the tip of `main` (or before a release is cut):

```bash
cargo install --git https://github.com/hoodiecollin/forgedb forgedb
```

This builds the whole workspace from source, so no crates need to be published
first.

## 6. Build from a clone

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
