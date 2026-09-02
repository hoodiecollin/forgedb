import type {
  Field,
  GridData,
  Model,
  Predicate,
  Relation,
  SavedQuery,
  Snapshot,
} from "./types";
export const MODELS: Model[] = [
  { key: "User", rows: "1,204", deadPct: 2.5, deadCount: "31", health: "ok", x: 250, y: 64, dataMB: "14.2", offMB: "1.1", reclaim: "31", idxCount: "3" },
  { key: "Post", rows: "8,932", deadPct: 6, deadCount: "541", health: "ok", x: 460, y: 62, dataMB: "96.4", offMB: "7.8", reclaim: "541", idxCount: "2" },
  { key: "Tag", rows: "47", deadPct: 0, deadCount: "0", health: "ok", x: 470, y: 252, dataMB: "0.1", offMB: "0.0", reclaim: "0", idxCount: "1" },
  { key: "Comment", rows: "44,120", deadPct: 18, deadCount: "7,941", health: "warn", x: 250, y: 300, dataMB: "61.0", offMB: "9.2", reclaim: "7,941", idxCount: "1" },
  { key: "Org", rows: "62", deadPct: 1, deadCount: "1", health: "ok", x: 48, y: 44, dataMB: "0.2", offMB: "0.0", reclaim: "1", idxCount: "1" },
];

export const REL: Record<string, Relation[]> = {
  User: [
    { kind: "has-many", label: "posts → Post", to: "Post", k: "hm" },
    { kind: "∗FK", label: "org → Org", to: "Org", k: "fk" },
    { kind: "↔ M2M", label: "tags ↔ Tag", to: "Tag", k: "m2m" },
  ],
  Post: [
    { kind: "∗FK", label: "author → User", to: "User", k: "fk" },
    { kind: "↔ M2M", label: "tags ↔ Tag", to: "Tag", k: "m2m" },
    { kind: "has-many", label: "comments → Comment", to: "Comment", k: "hm" },
  ],
  Tag: [{ kind: "↔ M2M", label: "posts ↔ Post", to: "Post", k: "m2m" }],
  Comment: [
    { kind: "∗FK", label: "post → Post", to: "Post", k: "fk" },
    { kind: "?FK", label: "author → User", to: "User", k: "fk" },
  ],
  Org: [{ kind: "has-many", label: "users → User", to: "User", k: "hm" }],
};
export const SCHEMA: Record<string, Field[]> = {
  User: [
    { name: "id", typeLabel: "uuid", mods: ["+", "&", "^"], control: "uuid", value: "7b10a4c2-9f3d-4e21-b8a0-2c1e44f1" },
    { name: "email", typeLabel: "string", mods: ["&", "^"], directive: "@email", control: "string", value: "lin@forge.dev", placeholder: "name@domain" },
    { name: "name", typeLabel: "string", mods: [], control: "string", value: "Lin Mercado" },
    { name: "bio", typeLabel: "string", mods: ["?"], directive: "@length(0, 280)", control: "text", value: "Infra & tooling. Owns the storage layer." },
    { name: "age", typeLabel: "u32", mods: ["?"], directive: "@min(0) @max(120)", control: "int", min: 0, max: 120, value: "34" },
    { name: "is_admin", typeLabel: "bool", mods: ["?"], control: "bool", default: "true" },
    { name: "role", typeLabel: "string", mods: [], directive: '@default("member")', control: "string", value: "member" },
    { name: "score", typeLabel: "f64", mods: [], control: "float", value: "87.5" },
    { name: "login_code", typeLabel: "bytes(8)", mods: [], control: "bytes", len: 8, value: "A1B2C3D4" },
    { name: "created_at", typeLabel: "timestamp", mods: ["+"], control: "ts", msVal: "1730556180000", humanVal: "2024-11-02 14:03" },
    { name: "org", typeLabel: "∗Org", mods: [], control: "fk", fkTarget: "Org", fkCurrent: "acme", fkOptions: [{ v: "acme", label: "Acme Inc" }, { v: "beta", label: "Beta LLC" }, { v: "globex", label: "Globex" }] },
    { name: "address", typeLabel: "struct", mods: ["?"], control: "struct", structFields: [{ name: "street", value: "12 Oak St", ph: "street" }, { name: "city", value: "Austin", ph: "city" }, { name: "zip", value: "78701", ph: "zip" }] },
    { name: "posts", typeLabel: "[Post]", mods: [], control: "hasmany", target: "Post", relCount: "18" },
    { name: "tags", typeLabel: "[Tag]", mods: [], control: "m2m", target: "Tag", chips: [{ label: "infra" }, { label: "rust" }, { label: "ops" }] },
  ],
  Post: [
    { name: "id", typeLabel: "uuid", mods: ["+", "&", "^"], control: "uuid", value: "a3f0-…-9c" },
    { name: "title", typeLabel: "string", mods: [], directive: "@max(140)", control: "string", value: "Append-only storage internals" },
    { name: "body", typeLabel: "string", mods: [], directive: "@fulltext", control: "text", value: "ForgeDB appends new record versions; a compaction step reclaims space." },
    { name: "status", typeLabel: "string", mods: [], directive: '@default("draft")', control: "string", value: "published" },
    { name: "views", typeLabel: "u64", mods: [], control: "bigint", value: "1840293" },
    { name: "published_at", typeLabel: "timestamp", mods: ["?"], control: "ts", msVal: "1730000000000", humanVal: "2024-10-27 03:20" },
    { name: "author", typeLabel: "∗User", mods: [], control: "fk", fkTarget: "User", fkCurrent: "lin", fkOptions: [{ v: "lin", label: "Lin Mercado" }, { v: "ada", label: "Ada Okafor" }, { v: "max", label: "Max Reyes" }] },
    { name: "tags", typeLabel: "[Tag]", mods: [], control: "m2m", target: "Tag", chips: [{ label: "storage" }, { label: "internals" }] },
    { name: "comments", typeLabel: "[Comment]", mods: [], control: "hasmany", target: "Comment", relCount: "42" },
  ],
  Tag: [
    { name: "id", typeLabel: "uuid", mods: ["+", "&", "^"], control: "uuid", value: "ta7-…-01" },
    { name: "name", typeLabel: "string", mods: ["&", "^"], control: "string", value: "infra" },
    { name: "color", typeLabel: "bytes(6)", mods: [], control: "bytes", len: 6, value: "A855F7" },
    { name: "posts", typeLabel: "[Post]", mods: [], control: "m2m", target: "Post", chips: [{ label: "312 linked" }] },
  ],
  Comment: [
    { name: "id", typeLabel: "uuid", mods: ["+", "&", "^"], control: "uuid", value: "cm9-…-b0" },
    { name: "body", typeLabel: "string", mods: [], control: "text", value: "Great write-up on compaction." },
    { name: "created_at", typeLabel: "timestamp", mods: ["+"], control: "ts", msVal: "1731020000000", humanVal: "2024-11-08 01:13" },
    { name: "post", typeLabel: "∗Post", mods: [], control: "fk", fkTarget: "Post", fkCurrent: "p1", fkOptions: [{ v: "p1", label: "Append-only storage internals" }, { v: "p2", label: "Schema-first codegen" }] },
    { name: "author", typeLabel: "?User", mods: ["?"], control: "fk", fkTarget: "User", fkCurrent: "ada", fkOptions: [{ v: "ada", label: "Ada Okafor" }, { v: "lin", label: "Lin Mercado" }] },
  ],
  Org: [
    { name: "id", typeLabel: "uuid", mods: ["+", "&", "^"], control: "uuid", value: "or2-…-77" },
    { name: "name", typeLabel: "string", mods: ["&", "^"], control: "string", value: "Acme Inc" },
    { name: "plan", typeLabel: "string", mods: [], directive: '@default("free")', control: "string", value: "pro" },
    { name: "seats", typeLabel: "u32", mods: [], control: "int", min: 1, value: "25" },
  ],
};
export const GRID: Record<string, GridData> = {
  User: {
    cols: [{ k: "id", l: "id · uuid", mono: true }, { k: "email", l: "email &^", mono: true }, { k: "name", l: "name" }, { k: "role", l: "role" }, { k: "age", l: "age", mono: true }, { k: "created", l: "created_at", mono: true }, { k: "org", l: "org →", rel: true }],
    rows: [
      { _id: "u1", id: "7b10…44", email: "lin@forge.dev", name: "Lin Mercado", role: "member", age: "34", created: "2024-11-02", org: "Acme Inc" },
      { _id: "u2", id: "3f2a…9c", email: "ada@forge.dev", name: "Ada Okafor", role: "admin", age: "41", created: "2024-10-28", org: "Acme Inc" },
      { _id: "u3", id: "c091…af", email: null, name: "Max Reyes", role: "member", age: "29", created: "2024-10-19", org: "Beta LLC" },
      { _id: "u4", id: "a55e…12", email: "sol@forge.dev", name: "Sol Nakamura", role: "member", age: "37", created: "2024-10-11", org: "Globex" },
      { _id: "u5", id: "de77…b0", email: "ivy@forge.dev", name: "Ivy Chen", role: "member", age: "26", created: "2024-10-02", org: "Acme Inc" },
      { _id: "u6", id: "f210…7e", email: "ken@forge.dev", name: "Ken Adebayo", role: "admin", age: "52", created: "2024-09-24", org: "Beta LLC" },
    ],
  },
  Post: {
    cols: [{ k: "id", l: "id", mono: true }, { k: "title", l: "title" }, { k: "status", l: "status" }, { k: "views", l: "views u64", mono: true }, { k: "author", l: "author →", rel: true }, { k: "published", l: "published_at", mono: true }],
    rows: [
      { _id: "p1", id: "a3f0…9c", title: "Append-only storage internals", status: "published", views: "1,840,293", author: "Lin Mercado", published: "2024-10-27" },
      { _id: "p2", id: "b7c1…22", title: "Schema-first codegen", status: "published", views: "902,145", author: "Ada Okafor", published: "2024-10-14" },
      { _id: "p3", id: "c9d2…41", title: "Draft: live queries", status: "draft", views: "0", author: "Lin Mercado", published: null },
      { _id: "p4", id: "d1e3…88", title: "Time-travel with snapshots", status: "review", views: "55,120", author: "Max Reyes", published: "2024-09-30" },
    ],
  },
  Tag: {
    cols: [{ k: "id", l: "id", mono: true }, { k: "name", l: "name &^" }, { k: "color", l: "color bytes(6)", mono: true }, { k: "posts", l: "posts ↔", rel: true }],
    rows: [
      { _id: "t1", id: "ta7…01", name: "infra", color: "A855F7", posts: "312 linked" },
      { _id: "t2", id: "ta8…02", name: "rust", color: "F97316", posts: "208 linked" },
      { _id: "t3", id: "ta9…03", name: "storage", color: "22C55E", posts: "91 linked" },
    ],
  },
  Comment: {
    cols: [{ k: "id", l: "id", mono: true }, { k: "body", l: "body" }, { k: "author", l: "author →", rel: true }, { k: "post", l: "post →", rel: true }, { k: "created", l: "created_at", mono: true }],
    rows: [
      { _id: "c1", id: "cm9…b0", body: "Great write-up on compaction.", author: "Ada Okafor", post: "Append-only…", created: "2024-11-08" },
      { _id: "c2", id: "cm8…a1", body: "Does this handle NaN?", author: null, post: "Schema-first…", created: "2024-11-07" },
      { _id: "c3", id: "cm7…92", body: "+1 for time-travel", author: "Max Reyes", post: "Time-travel…", created: "2024-11-05" },
    ],
  },
  Org: {
    cols: [{ k: "id", l: "id", mono: true }, { k: "name", l: "name &^" }, { k: "plan", l: "plan" }, { k: "seats", l: "seats u32", mono: true }],
    rows: [
      { _id: "o1", id: "or2…77", name: "Acme Inc", plan: "pro", seats: "25" },
      { _id: "o2", id: "or3…81", name: "Beta LLC", plan: "free", seats: "5" },
      { _id: "o3", id: "or4…90", name: "Globex", plan: "enterprise", seats: "140" },
    ],
  },
};
export const SAVED: SavedQuery[] = [
  { name: "active users · 30d", model: "User" },
  { name: "orphan posts", model: "Post" },
  { name: "admins by org", model: "User" },
];
export const TAILS = [
  { name: "new comments", k: "ok" as const },
  { name: "edits → Post", k: "info" as const },
];
export const SNAPS: Snapshot[] = [
  { name: "before-migration", time: "Nov 8 · 14:03" },
  { name: "nightly backup", time: "Nov 8 · 03:00" },
  { name: "pre-compaction", time: "Nov 6 · 22:10" },
];
export const POST_STATUS = [
  { label: "published", pct: 72, n: "6,431" },
  { label: "draft", pct: 19, n: "1,697" },
  { label: "review", pct: 9, n: "804" },
];
export const DEFAULT_PREDICATES: Predicate[] = [
  { field: "email", op: "=", val: '"lin@forge.dev"', idx: true },
  { field: "created_at", op: "≥", val: "1727740800000", idx: true },
];
export const DB_NAME = "blog.forge";
