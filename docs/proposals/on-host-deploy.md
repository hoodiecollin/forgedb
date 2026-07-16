# Proposal: Non-Docker (on-host) serve — landscape + initial supported set

**Status:** LANDED (2026-07-15). `forgedb-product-manager` verdict: **ALIGNED** (5 binding invariants, all honored; 2026-07-15) — systemd emission is the same class as the blessed Docker path; the initial slice adds no Rust dep, no generated-code change, no `main.rs` change.
**Issue:** [#115](https://github.com/hoodiecollin/forgedb/issues/115) (`documentation` + `idea`) — "Non-Docker (on-host) serve: outline the deployment landscape + pick the initial supported set"
**Date:** 2026-07-15

## Summary

Phase 5 WS2 (#93) shipped a **Docker-first** deploy story: `forgedb init` emits a
multi-stage `Dockerfile`, `.dockerignore`, and `docker-compose.yml`, plus a 12-factor
env-driven `main.rs` with a `HEALTHCHECK` on the generated `/health` route. Many operators
run the binary **directly under an init system** rather than in a container. This note maps
the on-host landscape, then picks a **deliberate first slice**: `forgedb init` also emits a
**systemd unit template + an `EnvironmentFile`** — the symmetric on-host artifact to the
Dockerfile — with every other init system documented-but-not-scaffolded.

The generated app is already the ideal on-host citizen: a single self-contained axum binary,
all config from the environment (no runtime config file), graceful drain on `SIGINT`/`SIGTERM`,
structured logging honoring `RUST_LOG` (text or `FORGEDB_LOG_FORMAT=json`), and three
unauthenticated ops routes (`/health`, `/ready`, `/metrics`). On-host deployment is therefore
**pure ops packaging around the existing binary** — no new Rust dependency, no `main.rs`
change, nothing that reads `schema.forge` at runtime. It is the exact same class of artifact
as the blessed Docker path.

## What already exists (nothing to build in the binary)

The generated `src/main.rs` (from `forgedb init`) already provides everything the initial
slice needs to wire into systemd without touching Rust:

- **Env-only config** — `FORGEDB_HOST` / `FORGEDB_PORT` / `FORGEDB_DATA` / `FORGEDB_TENANT` /
  `RUST_LOG` / `FORGEDB_LOG_FORMAT` and the `FORGEDB_JWT_*` guard knobs. All resolved once at
  startup; nothing is read from disk config. → maps 1:1 to a systemd `EnvironmentFile=`.
- **Graceful shutdown** — `axum::serve(..).with_graceful_shutdown(shutdown_signal())` resolves
  on `SIGINT` **or** `SIGTERM` and drains in-flight requests. `SIGTERM` is exactly how systemd
  stops a service (`ExecStop` default / `systemctl stop`). → clean `KillMode`/`TimeoutStopSec`
  behavior, no config needed.
- **Structured logging to stdout/stderr** — `tracing-subscriber` writes to the console; systemd
  captures stdout/stderr into the journal automatically. `FORGEDB_LOG_FORMAT=json` yields one
  JSON object per line for journald / a downstream shipper. → no logging wiring needed.
- **HTTP readiness** — `GET /ready` acquires a read lock and returns 200; `GET /health` is
  DB-free liveness. A reverse proxy or load balancer polls these directly.

The one thing the binary does **not** expose today is a **systemd `sd_notify` readiness
handshake** (`Type=notify` + `READY=1` on the notify socket). That would require a Rust
dependency (`sd-notify`) and a `main.rs` change to signal readiness after `bind`. It is
therefore **out of the initial slice** (see §Deferred) — HTTP `/ready` is the portable
readiness signal and covers proxy/LB health checks without it.

## The on-host landscape

| Init system / supervisor | Platform | Fit | Initial slice? |
|---|---|---|---|
| **systemd** | Linux (Debian/Ubuntu/RHEL/Fedora/Arch/SUSE — the default on ~all modern server distros) | **Best fit.** Native `EnvironmentFile=`, `StateDirectory=` (managed data dir + perms), `DynamicUser=` (non-root without a manual useradd), sandboxing directives, `Restart=`, journald logging. | **YES — scaffolded** |
| **OpenRC** | Alpine, Gentoo | Good fit. `/etc/init.d` script + `/etc/conf.d` env; `supervise-daemon` for restart. No `StateDirectory`/`DynamicUser` equivalent — manual user + `install -d -o`. | Documented |
| **runit** | Void, Alpine (alt), Docker-less minimal | Good fit. A `run` script `exec`ing the binary with `chpst -u` for the user + env dir; auto-restart is intrinsic. | Documented |
| **s6 / s6-rc** | minimal/embedded, some container inits | Good fit, same shape as runit (`run` script, `s6-setuidgid`). More moving parts (service dirs). | Documented |
| **launchd** | macOS | Fit for dev / single-node macOS. A `.plist` with `ProgramArguments` + `EnvironmentVariables` + `KeepAlive`. No Linux sandboxing analogue. | Documented |
| **Windows Service** | Windows | Works but needs a wrapper — the binary is a console app, not a native service. Use [WinSW](https://github.com/winsw/winsw) or `sc.exe` + a shim, or NSSM. `SIGTERM` semantics differ (Windows sends `CTRL_CLOSE`; the graceful-shutdown path is Unix-signal-oriented — `ctrl_c` fires, `SIGTERM` branch is `cfg(unix)`-gated and compiles to `pending()` on Windows). | Documented |
| **supervisord** | any (Python) — the systemd-less path | Fit for shared hosts / non-root PID-1-less setups. A `[program:x]` block with `environment=`, `autorestart=true`, `stopsignal=TERM`. | Documented |
| **nohup / tmux / screen** | any | **Not a deployment target.** No restart, no boot persistence, no log management. Explicitly discouraged in the docs. | Anti-pattern |

## Chosen initial slice: systemd unit template + EnvironmentFile

`forgedb init` (Rust scaffold path — same gate as the Docker emission) additionally writes a
`deploy/` directory:

- **`deploy/<name>.service`** — a systemd unit template.
- **`deploy/<name>.env`** — an `EnvironmentFile` with the documented `FORGEDB_*` knobs
  (commented defaults, uncomment to change), mirroring the compose `environment:` block.
- **`deploy/README.md`** — the copy/enable/start steps + the install prerequisite (build the
  binary, copy it to `/usr/local/bin`).

systemd goes in a `deploy/` subdirectory (not the project root, unlike `Dockerfile`): a unit
file has no build-context root requirement — it is installed to `/etc/systemd/system/` — and
grouping the on-host artifacts keeps the root uncluttered as more are added.

### The unit file (shape)

```ini
[Unit]
Description=<name> — ForgeDB generated server
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
# Build with `cargo build --release` and install the binary here:
ExecStart=/usr/local/bin/<name>
EnvironmentFile=/etc/<name>/<name>.env

# Non-root without a manual useradd; systemd allocates a transient user and
# persists the state dir across restarts with a stable-enough identity.
DynamicUser=yes
# Managed data dir → /var/lib/<name>, created + chowned to the service user.
# The env file sets FORGEDB_DATA=/var/lib/<name> to match.
StateDirectory=<name>

Restart=on-failure
RestartSec=2
# main.rs drains in-flight requests on SIGTERM (graceful shutdown).
KillSignal=SIGTERM
TimeoutStopSec=30

# Hardening — the server needs only its state dir + a TCP socket.
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictAddressFamilies=AF_INET AF_INET6
RestrictNamespaces=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
```

Rationale for the load-bearing choices:

- **`Type=exec`** (not `notify`): the binary does not `sd_notify`, so `exec` (started once the
  binary successfully execs) is the honest readiness model. Proxy/LB readiness is `GET /ready`.
- **`DynamicUser=yes` + `StateDirectory=`**: the on-host analogue of the container's non-root
  user + `/data` VOLUME — non-root, isolated, persistent data dir, **without** requiring the
  operator to `useradd` and `chown`. `StateDirectory=<name>` creates `/var/lib/<name>` owned by
  the service, and the emitted env file sets `FORGEDB_DATA` to it, so it works out of the box.
- **`KillSignal=SIGTERM` + `TimeoutStopSec=30`**: pairs with the existing graceful-shutdown
  path so `systemctl stop` drains rather than severs.
- **Hardening block**: the server touches only its state dir and a listening socket, so the
  standard sandboxing directives apply cleanly; `StateDirectory` stays writable under
  `ProtectSystem=strict`.

### The single-writer contract in a unit file

The v1 single-writer-per-data-dir contract (advisory `DirLock` on open) matters on-host: the
unit must not be templated (`<name>@.service`) in a way that invites two instances against the
same `FORGEDB_DATA`. The emitted unit is a **single** service; the docs explain that a second
process against the same data dir refuses to start (does not corrupt), and that per-tenant
scale-out means **one unit per tenant** each with its own `FORGEDB_TENANT` + state dir (the
on-host mirror of process-per-tenant). A systemd **template** unit (`<name>@.service` keyed on
the tenant) is documented as the multi-tenant pattern but the initial scaffold emits the
single plain unit for clarity.

## Reverse proxy / TLS (BYO)

The old bundled-nginx model is gone. On-host, TLS termination + host/subdomain routing is the
operator's reverse proxy (nginx or Caddy) in front of the bound port. The docs give a minimal
Caddy and nginx stanza (proxy to `127.0.0.1:3000`, forward the `Upgrade`/`Connection` headers
so the change-feed / live-query / replication **WebSocket** routes work, forward
`Authorization` for the JWT guard). This is documentation, not a scaffolded artifact — proxy
config is too environment-specific to bless one.

## Product verdict & invariant mapping

This is **class-2 ops packaging** around the already-generated server binary — the same class
as the Docker WS2 path that already shipped. The unit file and env file:

- read `schema.forge` **never** (they name the binary + env vars, nothing schema-derived beyond
  the project name, exactly like the Dockerfile);
- ship **no runtime** that interprets a schema — they start a binary whose data logic was
  generated at compile time;
- add **no dependency** to the generated crate and **no change** to generated code or the
  scaffold `main.rs` (the initial slice is inert `.service`/`.env`/`.md` text files).

The generator-identity invariant is untouched: schema is still a compile-time input; every
published artifact is still substrate or transport/ops glue. A `forgedb-product-manager` gate
should confirm the parity with the Docker path (expected verdict: aligned).

## Honest limits / deferred

- **No `sd_notify` readiness.** `Type=exec` marks "started" at exec, not at "listening +
  DB-open." True socket-notify readiness (`Type=notify` + `READY=1` after bind) needs an
  `sd-notify` dep + a `main.rs` signal — a separate slice with its own gate. HTTP `/ready`
  covers proxy/LB readiness in the meantime.
- **systemd only, scaffolded.** OpenRC/runit/s6/launchd/Windows/supervisord are documented with
  a fit note and a hand-portable template in the docs, not emitted by `init`. systemd covers the
  overwhelming majority of Linux server installs; the rest are a copy-paste away.
- **Binary install is manual.** The unit references `/usr/local/bin/<name>`; the operator builds
  (`cargo build --release`) and copies it there (the `deploy/README.md` says so). No packaging
  (`.deb`/`.rpm`) is generated — that is a larger, separate effort.
- **Reverse proxy is documented, not scaffolded.** Too environment-specific to bless one config.
- **Windows graceful shutdown** rides `ctrl_c` only (the `SIGTERM` branch is `cfg(unix)`); a
  Windows service wrapper's stop request maps to that path imperfectly — documented as a known
  rough edge, not a v1 scaffold target.

## Out of scope

- OS packaging (`.deb` / `.rpm` / Homebrew formula) for the generated app.
- A `forgedb serve` supervisor / process manager built into the CLI.
- `sd_notify` / socket-activation readiness (deferred slice above).

## Build order

1. **Design note (this) + PM gate.**
2. **Codegen/CLI — `create_deploy_files` in `src/commands/init.rs`** additionally writes
   `deploy/<name>.service`, `deploy/<name>.env`, `deploy/README.md` alongside the Docker
   artifacts (Rust-scaffold path). An integration test asserts the files emit with the expected
   directives; `systemd-analyze verify` in the E2E if available.
3. **Docs — `docs/DEPLOYMENT.md`** gains an "On-host (systemd)" section: install/enable/start,
   the `EnvironmentFile` knobs, per-tenant units, BYO reverse-proxy/TLS stanzas, and the
   enumerated other-init-systems table with hand-portable templates.
</content>
</invoke>
