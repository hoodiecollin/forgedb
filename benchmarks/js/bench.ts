//! JS/Bun benchmark suite: PGlite (PostgreSQL 16 compiled to WASM, run in-process,
//! no server) vs `bun:sqlite` (SQLite via Bun's native binding) — the two in-process
//! engines that live in JS rather than the native Rust Criterion harness. Reading
//! PGlite (in-process WASM PG) against the native server-PostgreSQL numbers
//! (`make bench-postgres`) separates the *engine* cost from the *client/server
//! transport* cost — that contrast is the point of including PGlite.
//!
//! The corpus generator mirrors the Rust `benchmarks/src/lib.rs` seed logic
//! (splitmix64 + id_for) so the data model matches the native suites; the read
//! corpus is smaller (2 000 posts) because PGlite's per-query WASM-boundary cost
//! makes a 1e4 row-at-a-time load slow — point/probe latency is O(1), so the
//! smaller corpus is representative for those. Run: `bun run benchmarks/js/bench.ts`
//! (or `make bench-pglite`).

import { PGlite } from "@electric-sql/pglite";
import { Database as Sqlite } from "bun:sqlite";
import { run, bench, group, summary } from "mitata";

// ---- Deterministic corpus (mirrors benchmarks/src/lib.rs) --------------------
const BASE_TS = 1_700_000_000n;
const N_TAGS = 500;
const TAGS_PER_POST = 3;
const READ_USERS = 1_000;
const READ_POSTS = 2_000;
const MASK64 = (1n << 64n) - 1n;

function splitmix64(state: { s: bigint }): bigint {
  state.s = (state.s + 0x9e3779b97f4a7c15n) & MASK64;
  let z = state.s;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK64;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK64;
  return (z ^ (z >> 31n)) & MASK64;
}

/** 16-byte big-endian id from (kind << 96) | index — matches Uuid::from_u128. */
function idFor(kind: number, index: number): Uint8Array {
  let v = (BigInt(kind) << 96n) | BigInt(index);
  const b = new Uint8Array(16);
  for (let i = 15; i >= 0; i--) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

interface UserRow { id: Uint8Array; name: string; email: string; created_at: bigint; }
interface PostRow { id: Uint8Array; title: string; views: bigint; published: boolean; author: Uint8Array; created_at: bigint; }
interface TagRow { id: Uint8Array; name: string; }

function dataset(nUsers: number, nPosts: number) {
  const nTags = Math.min(N_TAGS, Math.max(nPosts, 1));
  const rng = { s: 0x123456789abcdef0n };
  const users: UserRow[] = [];
  for (let i = 0; i < nUsers; i++)
    users.push({ id: idFor(1, i), name: `user${i}`, email: `user${i}@example.com`, created_at: BASE_TS + BigInt(i) });
  const posts: PostRow[] = [];
  for (let i = 0; i < nPosts; i++)
    posts.push({
      id: idFor(2, i), title: `post title number ${i}`,
      views: splitmix64(rng) % 100_000n, published: i % 2 === 0,
      author: idFor(1, i % Math.max(nUsers, 1)), created_at: BASE_TS + BigInt(i),
    });
  const tags: TagRow[] = [];
  for (let i = 0; i < nTags; i++) tags.push({ id: idFor(3, i), name: `tag${i}` });
  const links: [number, number][] = [];
  for (let p = 0; p < nPosts; p++)
    for (let k = 0; k < TAGS_PER_POST; k++) links.push([p, (p * 7 + k * 101) % nTags]);
  return { users, posts, tags, links };
}

const DDL_PG = `
CREATE TABLE "user" (id BYTEA PRIMARY KEY, name TEXT, email TEXT UNIQUE, created_at BIGINT);
CREATE TABLE post (id BYTEA PRIMARY KEY, title TEXT, views BIGINT, published BOOLEAN, author BYTEA, created_at BIGINT);
CREATE INDEX post_author_idx ON post(author);
CREATE TABLE tag (id BYTEA PRIMARY KEY, name TEXT);
CREATE TABLE post_tag_link (post_id BYTEA, tag_id BYTEA);
CREATE INDEX ptl_post_idx ON post_tag_link(post_id);
`;

const DDL_SQLITE = `
CREATE TABLE user (id BLOB PRIMARY KEY, name TEXT, email TEXT UNIQUE, created_at INTEGER);
CREATE TABLE post (id BLOB PRIMARY KEY, title TEXT, views INTEGER, published INTEGER, author BLOB, created_at INTEGER);
CREATE INDEX post_author_idx ON post(author);
CREATE TABLE tag (id BLOB PRIMARY KEY, name TEXT);
CREATE TABLE post_tag_link (post_id BLOB, tag_id BLOB);
CREATE INDEX ptl_post_idx ON post_tag_link(post_id);
`;

const data = dataset(READ_USERS, READ_POSTS);

// ---- PGlite setup (in-process WASM Postgres) --------------------------------
async function setupPglite(): Promise<PGlite> {
  const db = new PGlite();
  await db.exec(DDL_PG);
  await db.exec("BEGIN");
  for (const u of data.users)
    await db.query('INSERT INTO "user" (id,name,email,created_at) VALUES ($1,$2,$3,$4)', [u.id, u.name, u.email, u.created_at]);
  for (const p of data.posts)
    await db.query("INSERT INTO post (id,title,views,published,author,created_at) VALUES ($1,$2,$3,$4,$5,$6)", [p.id, p.title, p.views, p.published, p.author, p.created_at]);
  for (const t of data.tags)
    await db.query("INSERT INTO tag (id,name) VALUES ($1,$2)", [t.id, t.name]);
  for (const [p, t] of data.links)
    await db.query("INSERT INTO post_tag_link (post_id,tag_id) VALUES ($1,$2)", [data.posts[p].id, data.tags[t].id]);
  await db.exec("COMMIT");
  return db;
}

// ---- bun:sqlite setup (native SQLite) ---------------------------------------
function setupSqlite(): Sqlite {
  const db = new Sqlite(":memory:");
  db.run(DDL_SQLITE);
  const tx = db.transaction(() => {
    const iu = db.query("INSERT INTO user (id,name,email,created_at) VALUES (?,?,?,?)");
    for (const u of data.users) iu.run(u.id, u.name, u.email, Number(u.created_at));
    const ip = db.query("INSERT INTO post (id,title,views,published,author,created_at) VALUES (?,?,?,?,?,?)");
    for (const p of data.posts) ip.run(p.id, p.title, Number(p.views), p.published ? 1 : 0, p.author, Number(p.created_at));
    const it = db.query("INSERT INTO tag (id,name) VALUES (?,?)");
    for (const t of data.tags) it.run(t.id, t.name);
    const il = db.query("INSERT INTO post_tag_link (post_id,tag_id) VALUES (?,?)");
    for (const [p, t] of data.links) il.run(data.posts[p].id, data.tags[t].id);
  });
  tx();
  return db;
}

const pg = await setupPglite();
const sq = setupSqlite();

// Prepared PGlite statements (parse once, mirror the native pg suite).
const PG_POINT = "SELECT id,title,views,published,author,created_at FROM post WHERE id = $1";
const PG_EMAIL = 'SELECT id,name,email,created_at FROM "user" WHERE email = $1';
const PG_AUTHOR = "SELECT id,title,views,published,author,created_at FROM post WHERE author = $1";
const PG_TAGS = "SELECT tag.id, tag.name FROM tag JOIN post_tag_link l ON l.tag_id = tag.id WHERE l.post_id = $1";
// Scenario 7: filtered scan + aggregate, and filtered scan + sort + top-N.
const PG_AGG = "SELECT COUNT(*), COALESCE(SUM(views),0) FROM post WHERE published";
const PG_TOPN = "SELECT id,title,views,published,author,created_at FROM post WHERE views >= $1 ORDER BY views DESC LIMIT 10";

// Prepared bun:sqlite statements.
const sqPoint = sq.query("SELECT id,title,views,published,author,created_at FROM post WHERE id = ?");
const sqEmail = sq.query("SELECT id,name,email,created_at FROM user WHERE email = ?");
const sqAuthor = sq.query("SELECT id,title,views,published,author,created_at FROM post WHERE author = ?");
const sqTags = sq.query("SELECT tag.id, tag.name FROM tag JOIN post_tag_link l ON l.tag_id = tag.id WHERE l.post_id = ?");
const sqAgg = sq.query("SELECT COUNT(*), COALESCE(SUM(views),0) FROM post WHERE published = 1");
const sqTopN = sq.query("SELECT id,title,views,published,author,created_at FROM post WHERE views >= ? ORDER BY views DESC LIMIT 10");

let i = 0;

// Scenario 5: point lookup by PK.
summary(() => {
  group("js/point_lookup", () => {
    bench("pglite", async () => { const id = idFor(2, i++ % READ_POSTS); await pg.query(PG_POINT, [id]); });
    bench("bun:sqlite", () => { const id = idFor(2, i++ % READ_POSTS); sqPoint.get(id); });
  });
});

// Scenario 6: secondary-index probe (unique email).
summary(() => {
  group("js/index_probe", () => {
    bench("pglite", async () => { await pg.query(PG_EMAIL, [`user${i++ % READ_USERS}@example.com`]); });
    bench("bun:sqlite", () => { sqEmail.get(`user${i++ % READ_USERS}@example.com`); });
  });
});

// Scenario 8/10: FK-index probe -> reverse one-to-many.
summary(() => {
  group("js/reverse_fk", () => {
    bench("pglite", async () => { const id = idFor(1, i++ % READ_USERS); await pg.query(PG_AUTHOR, [id]); });
    bench("bun:sqlite", () => { const id = idFor(1, i++ % READ_USERS); sqAuthor.all(id); });
  });
});

// Scenario 11: many-to-many traversal (indexed junction join).
summary(() => {
  group("js/m2m", () => {
    bench("pglite", async () => { const id = idFor(2, i++ % READ_POSTS); await pg.query(PG_TAGS, [id]); });
    bench("bun:sqlite", () => { const id = idFor(2, i++ % READ_POSTS); sqTags.all(id); });
  });
});

// Scenario 7a: filtered scan + aggregate (COUNT + SUM(views) WHERE published).
summary(() => {
  group("js/scan_aggregate", () => {
    bench("pglite", async () => { await pg.query(PG_AGG); });
    bench("bun:sqlite", () => { sqAgg.get(); });
  });
});

// Scenario 7b: filtered scan + sort + top-10 by views.
summary(() => {
  group("js/scan_sort_top10", () => {
    bench("pglite", async () => { await pg.query(PG_TOPN, [50_000]); });
    bench("bun:sqlite", () => { sqTopN.all(50_000); });
  });
});

await run();
await pg.close();
sq.close();
