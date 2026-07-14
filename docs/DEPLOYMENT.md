# Deploying a ForgeDB App

A ForgeDB-generated app is a single self-contained axum binary. `forgedb init`
emits a blessed container path (multi-stage `Dockerfile`, `.dockerignore`,
`docker-compose.yml`) and a 12-factor `main.rs` configured entirely from the
environment. This guide covers running it in production.

> New here? Do the [Getting Started](./GETTING_STARTED.md) loop first, then come
> back to deploy.

## The binary

The generated `src/main.rs` reads all config from the environment (no config
file at runtime), opens its data directory, and serves the REST + WebSocket API
plus the operational routes. It installs a `tracing` subscriber and drains
in-flight requests on `SIGINT`/`SIGTERM` (graceful shutdown), so a container
stop or `Ctrl-C` never severs a connection mid-write.

### Environment variables

| Var | Default | Purpose |
|---|---|---|
| `FORGEDB_HOST` | `127.0.0.1` | bind host — set `0.0.0.0` in a container |
| `FORGEDB_PORT` | `3000` | bind port |
| `FORGEDB_DATA` | `data` | data directory (the per-tenant root) |
| `FORGEDB_TENANT` | *(unset)* | tenant this process serves → opens `<FORGEDB_DATA>/<tenant>` |
| `RUST_LOG` | `info` | log level filter (`tracing-subscriber` env-filter) |
| `FORGEDB_LOG_FORMAT` | *(text)* | set `json` for machine-parseable log lines |

**JWT tenant guard** (verify-only — ForgeDB verifies tokens your IdP issues, it
never issues them). Enabled when `FORGEDB_JWT_PUBKEY` is set:

| Var | Default | Purpose |
|---|---|---|
| `FORGEDB_JWT_PUBKEY` | *(unset)* | path to the IdP's PEM public (verification) key |
| `FORGEDB_JWT_ALG` | `RS256` | signature algorithm (asymmetric only) |
| `FORGEDB_JWT_ISSUER` | *(unset)* | expected `iss` |
| `FORGEDB_JWT_AUDIENCE` | *(unset)* | expected `aud` |
| `FORGEDB_TENANT_CLAIM` | `tenant` | claim carrying the tenant id |
| `FORGEDB_JWT_LEEWAY` | `60` | clock-skew leeway (seconds) |

When the guard is on, `FORGEDB_TENANT` is required: the token's tenant claim is
cross-checked against it (wrong tenant → 403). See
[WHAT_V1_IS.md](./WHAT_V1_IS.md) for exactly what the auth layer does and does
not do (verify-only: no token issuance, no row-level authorization).

## Operational routes (no auth)

The router serves three ops routes **outside** the JWT guard, so load balancers
and Kubernetes probes reach them without a token:

| Route | Response | Use |
|---|---|---|
| `GET /health` | `{"status":"ok"}` | liveness — never touches the DB |
| `GET /ready` | `{"status":"ready"}` | readiness — acquires a read lock |
| `GET /metrics` | `{"model_count":N,"rows_per_model":{…},"total_rows":N}` | minimal per-model row counts |

Point your liveness probe at `/health` and your readiness probe at `/ready`.
`/metrics` is a minimal JSON scrape (not Prometheus text format in v1).

## Containers

`forgedb init` writes a ready-to-build image. The build context expects the
generated code present, so generate before building:

```bash
forgedb generate all --output ./generated
docker build -t myblog .
docker run -p 3000:3000 -v myblog-data:/data myblog
```

The generated `Dockerfile` is multi-stage:

- **builder** — `rust:1-slim`, copies manifests then sources, `cargo build
  --release --locked` (falls back to unlocked if the lockfile is absent).
- **runtime** — `debian:bookworm-slim` with only `ca-certificates` + `curl`, a
  non-root `forgedb` user (uid 10001), the binary at
  `/usr/local/bin/forgedb-server`.
- **config** — `FORGEDB_HOST=0.0.0.0`, `FORGEDB_PORT=3000`, `FORGEDB_DATA=/data`,
  `RUST_LOG=info` baked as defaults; `/data` is a `VOLUME` (never bake data into
  the image); `HEALTHCHECK` curls `/health`.

### Compose

```bash
docker compose up --build
```

The generated `docker-compose.yml` maps `3000:3000`, mounts a named
`forgedb-data` volume at `/data`, and includes the same `/health` healthcheck.
Tenancy, JSON logs, and JWT knobs are present as commented environment entries —
uncomment what you need.

## Multi-tenancy (process-per-tenant)

ForgeDB v1 multi-tenancy is **physical**: one `forgedb serve` / one generated
process serves **one** tenant's data directory. Each tenant's data lives under
`<FORGEDB_DATA>/<tenant>/`; you run N processes behind a host/subdomain proxy
that routes each tenant to its process.

```bash
forgedb tenant create acme --root ./data   # make ./data/acme
FORGEDB_TENANT=acme FORGEDB_DATA=./data ./target/release/myblog
```

Manage tenant directories with `forgedb tenant {create,list,drop}`. This is the
v1 contract; an in-process tenant registry (one process, many tenants) is a
deferred superset. See [WHAT_V1_IS.md](./WHAT_V1_IS.md).

## Data, durability, and the single-writer contract

- The data directory is the whole database. Back it up with
  `forgedb backup create` (a lock-free full snapshot); run `forgedb backup
  --help` for the `create`/`restore`/`list` subcommands. For full/incremental
  backups **and point-in-time recovery** (recover to a precise broker offset),
  see [BACKUP_RECOVERY.md](./BACKUP_RECOVERY.md).
- Writes are crash-safe: each mutation is journaled to a per-model WAL with an
  fsync **before** columns are touched, and recovery repairs a torn tail on
  reopen.
- **One writer per data directory.** The process takes an advisory directory
  lock on open; a second writer against the same directory refuses to start
  (it does not corrupt). Do **not** point two server processes at the same
  `FORGEDB_DATA/<tenant>` dir. Concurrent *readers* are fine.

Because of the single-writer contract, scale a single tenant **vertically** (one
process) and scale **across** tenants horizontally (one process each). See
[WHAT_V1_IS.md](./WHAT_V1_IS.md) for the full durability + concurrency scope.

## Logs

Structured logging is the standard stack — `tracing` + `tracing-subscriber`
(honors `RUST_LOG`) + a `tower-http` request span per request. Text by default;
`FORGEDB_LOG_FORMAT=json` emits one JSON object per line for a log aggregator.

## Releases / prebuilt binaries

Tagging a release (`git tag v* && git push --tags`) triggers
`.github/workflows/release.yml`, which cross-builds the `forgedb` **CLI** for
Linux (x86_64/aarch64), macOS (Intel/ARM), and Windows and attaches them to a
GitHub Release. (That workflow ships the CLI; your generated *app* is built from
its own `Dockerfile` as above.) Install paths for the CLI are in
[INSTALL.md](./INSTALL.md).
