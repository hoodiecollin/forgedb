#!/usr/bin/env bash
# Spin an EPHEMERAL local PostgreSQL cluster from the devbox-provided `postgresql`
# package (no binary download — declarative host dep), run the pg_bench suite
# against it over a unix socket, then tear the cluster down. Meant to be run under
# devbox so initdb/pg_ctl/postgres are on PATH:
#
#   make bench-postgres      # (wraps `devbox run -- benchmarks/scripts/pg_run.sh`)
#
# Extra args after `--` are forwarded to `cargo bench` (e.g. --measurement-time 3).
set -euo pipefail

command -v initdb >/dev/null || {
  echo "initdb not on PATH — run under devbox (\`devbox run -- $0\`) so the" >&2
  echo "devbox-provided postgresql package is available." >&2
  exit 127
}

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PGDATA="$(mktemp -d "${TMPDIR:-/tmp}/forgedb_pg.XXXXXX")"
SOCKDIR="$(mktemp -d "${TMPDIR:-/tmp}/forgedb_pgsock.XXXXXX")"
DBNAME="bench"

cleanup() {
  pg_ctl -D "$PGDATA" -m immediate stop >/dev/null 2>&1 || true
  rm -rf "$PGDATA" "$SOCKDIR"
}
trap cleanup EXIT

echo "==> initdb (ephemeral cluster at $PGDATA)"
initdb -D "$PGDATA" -A trust -U "$(id -un)" >/dev/null

echo "==> starting postgres (unix socket only, no TCP)"
# -k sets the socket dir; listen_addresses='' disables TCP. Durability knobs are
# left at defaults; the suite toggles synchronous_commit per group.
pg_ctl -D "$PGDATA" -o "-k $SOCKDIR -c listen_addresses=''" -w start >/dev/null

createdb -h "$SOCKDIR" -U "$(id -un)" "$DBNAME"

export FORGEDB_BENCH_PG_URL="host=$SOCKDIR user=$(id -un) dbname=$DBNAME"
echo "==> FORGEDB_BENCH_PG_URL=$FORGEDB_BENCH_PG_URL"

echo "==> running pg_bench"
cargo bench --manifest-path "$ROOT/benchmarks/Cargo.toml" --bench pg_bench -- "$@"

echo "==> done (cluster torn down on exit)"
