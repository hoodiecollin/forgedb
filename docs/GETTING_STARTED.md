# Getting Started with ForgeDB

This guide takes you from an empty directory to a running, type-safe database
server with a typed TypeScript client — the whole `init → generate → build →
serve` loop. Every command and output below is from a real run against the
published crates.

> **What ForgeDB is.** A *code generator*, not a runtime ORM. You write a
> declarative `.forge` schema; ForgeDB transpiles it into tailored Rust database
> code, a REST API, and a TypeScript SDK. Your schema is a **compile-time input
> to generation**, never a runtime input to a generic engine. Read the honest
> scope of what v1 does and doesn't do in [WHAT_V1_IS.md](./WHAT_V1_IS.md).

## 1. Install

```bash
cargo install forgedb
```

Other install paths (prebuilt binaries, `--git`, from a clone) are in
[INSTALL.md](./INSTALL.md). Verify:

```bash
forgedb --version      # forgedb 0.1.0
```

## 2. Scaffold a project

```bash
forgedb init myblog --template blog --rust
cd myblog
```

`--template` accepts `blog`, `ecommerce`, `todo`, or `blank` (the default).
`--rust` includes the Rust backend scaffold. `init` writes:

```
myblog/
  schema.forge          # your schema (the single source of truth)
  forgedb.toml          # project + database + api + codegen config
  Cargo.toml            # pins the schema-agnostic substrate crates
  src/main.rs           # env-driven axum server (tenancy, JWT, graceful shutdown)
  Dockerfile            # multi-stage build → slim runtime (see DEPLOYMENT.md)
  .dockerignore
  docker-compose.yml
  .gitignore
  README.md
```

The blog template's `schema.forge`:

```
User {
  id: +uuid
  username: ^&string
  email: ^&string @email
  password_hash: string
  created_at: +timestamp
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  slug: ^&string
  content: string
  published: bool
  published_at: timestamp?
  created_at: +timestamp
  updated_at: +timestamp
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  name: ^&string
  posts: [Post]
}
```

The modifiers: `+` auto-generate (uuid/timestamp), `&` unique, `^` index, `?`
nullable, `*User` a required foreign key, `[Post]` a one-to-many, `[..]/[..]` a
many-to-many. The full grammar is in [SCHEMA.md](./SCHEMA.md).

## 3. Generate code

```bash
forgedb generate rust         # → generated/database.rs
forgedb generate api          # → generated/api.rs (+ package.json, tsconfig.json)
forgedb generate node --sdk   # → generated/types.ts  (the REST SDK; `bun --sdk` is equivalent)
```

Or `forgedb generate all` for everything at once (adds the OpenAPI spec).
Output goes to `./generated/` by default (`--output` to change it). The Rust
generator emits one tailored `database.rs` per schema — typed structs, columnar
storage, indexes, relation traversal, validation, and a crash-safe write path —
none of it reflects over your schema at runtime.

## 4. Build

```bash
cargo build
```

The generated app links only the small, **schema-agnostic** substrate crates
(`forgedb-storage`, `forgedb-wal`, `forgedb-types`, …) that `init` pinned in
`Cargo.toml`; they resolve from crates.io. See the substrate version matrix in
[INSTALL.md](./INSTALL.md).

## 5. Run the server

The generated `main.rs` is a 12-factor axum server configured entirely from the
environment:

```bash
FORGEDB_PORT=3000 FORGEDB_DATA=./data ./target/debug/myblog
# INFO myblog: ForgeDB serving tenant=None data_root=./data addr=127.0.0.1:3000
```

Key environment variables (full list in [DEPLOYMENT.md](./DEPLOYMENT.md)):

| Var | Default | Purpose |
|---|---|---|
| `FORGEDB_HOST` | `127.0.0.1` | bind host (`0.0.0.0` in containers) |
| `FORGEDB_PORT` | `3000` | bind port |
| `FORGEDB_DATA` | `data` | data directory (per-tenant root) |
| `FORGEDB_TENANT` | *(unset)* | tenant this process serves (see multi-tenancy) |
| `FORGEDB_LOG_FORMAT` | *(text)* | `json` for machine-parseable log lines |

The server exposes **operational routes** that need no auth — designed for
load-balancer and Kubernetes probes:

```bash
curl localhost:3000/health    # {"status":"ok"}      — liveness (never touches the DB)
curl localhost:3000/ready     # {"status":"ready"}   — acquires a read lock
curl localhost:3000/metrics   # {"model_count":3,"rows_per_model":{"Post":0,"Tag":0,"User":0},"total_rows":0}
```

## 6. Use the REST API

Each model gets a REST resource under `/api/<model>`:

```bash
# Create — returns the new id (201)
curl -X POST localhost:3000/api/user -H 'content-type: application/json' -d '{
  "id":"11111111-1111-1111-1111-111111111111",
  "username":"ada","email":"ada@example.com","password_hash":"x",
  "created_at":0,"posts":null
}'
# → {"id":"11111111-1111-1111-1111-111111111111"}

# List — paginated envelope
curl localhost:3000/api/user
# → {"data":[{...}],"total":1,"limit":50,"offset":0}
```

Field validation is enforced at write and mapped to HTTP:

```bash
curl -X POST localhost:3000/api/user -H 'content-type: application/json' -d '{
  "id":"22222222-2222-2222-2222-222222222222","username":"bob",
  "email":"not-an-email","password_hash":"x","created_at":0,"posts":null
}'
# → 422 {"error":"field `email` violates `email`: must be a valid email address"}
```

`@email`/`@min`/`@max`/`@length`/`@url` violations return **422**; a `&unique`
collision or a dangling foreign key returns **409**.

> **v1 create contract.** The REST `create` body is deserialized into the whole
> record, so you supply every field — including auto (`+`) fields like `id` and
> `created_at` and any virtual relation fields (as `null`). The direct Rust
> `db.create_<model>` path *does* auto-generate `+` fields; the REST layer does
> not yet. This is a documented v1 limit ([WHAT_V1_IS.md](./WHAT_V1_IS.md)).

Full route set per model: `GET /api/<model>` (list, with
`?limit&offset&sort&<field>=`), `POST /api/<model>` (create),
`GET|PUT|DELETE /api/<model>/{id}`.

## 7. Use the typed TypeScript SDK

`forgedb generate node --sdk` (or `bun --sdk`) emits `generated/types.ts` plus a `package.json`
and `tsconfig.json` (only if absent — regeneration never clobbers your edits),
so it's npm-publishable as-is. The client is full CRUD, faithful to the REST
contract:

```ts
import { ForgeDBClient } from './generated/types';

const db = new ForgeDBClient('http://localhost:3000');

// list → ListResult<T> = { data, total, limit, offset }
const { data, total } = await db.listUser({ limit: 20, sort: 'username' });

// get → the row, or null on 404
const user = await db.getUser('11111111-1111-1111-1111-111111111111');

// create → the new id; throws ForgeDBError on 409/422
const id = await db.createUser({
  username: 'grace', email: 'grace@example.com',
  password_hash: 'x', created_at: Date.now(), posts: null,
});

// update → false if the id doesn't exist; delete → true/false
await db.updateUser(id, { /* full record */ });
await db.deleteUser(id);
```

Errors surface as a typed `ForgeDBError` carrying the HTTP status and parsed
body (except get/delete 404s, which return `null`/`false`).

## Next steps

- **[SCHEMA.md](./SCHEMA.md)** — the complete `.forge` language reference.
- **[examples/](../examples/README.md)** — 18 worked schemas across many domains.
- **[DEPLOYMENT.md](./DEPLOYMENT.md)** — containers, env config, ops routes, multi-tenancy, JWT.
- **[MIGRATIONS.md](./MIGRATIONS.md)** — how schema changes affect existing data.
- **[WHAT_V1_IS.md](./WHAT_V1_IS.md)** — the honest scope: guarantees and limits of v1.
