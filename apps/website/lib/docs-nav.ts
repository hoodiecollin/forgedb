export interface NavItem {
  title: string;
  href: string;
}
export interface NavGroup {
  title: string;
  items: NavItem[];
}
export const docsNav: NavGroup[] = [
  {
    title: "Introduction",
    items: [
      { title: "What is ForgeDB", href: "/docs/" },
      { title: "What pre-1.0 is (and isn't)", href: "/docs/what-pre-1-0-is/" },
      { title: "Core concepts", href: "/docs/concepts/" },
      { title: "Benchmarks", href: "/docs/benchmarks/" },
      { title: "Installation", href: "/docs/installation/" },
      { title: "Quickstart", href: "/docs/quickstart/" },
    ],
  },
  {
    title: "Schema language",
    items: [
      { title: "Overview", href: "/docs/schema/overview/" },
      { title: "Models & fields", href: "/docs/schema/models-and-fields/" },
      { title: "Scalar types", href: "/docs/schema/scalar-types/" },
      { title: "Modifiers", href: "/docs/schema/modifiers/" },
      { title: "Relations", href: "/docs/schema/relations/" },
      { title: "Enums", href: "/docs/schema/enums/" },
      { title: "Directives", href: "/docs/schema/directives/" },
      { title: "Indexes & projections", href: "/docs/schema/indexes-and-projections/" },
      { title: "Comments & whitespace", href: "/docs/schema/comments/" },
      { title: "Cheatsheet", href: "/docs/schema/cheatsheet/" },
    ],
  },
  {
    title: "CLI reference",
    items: [
      { title: "Overview", href: "/docs/cli/overview/" },
      { title: "init", href: "/docs/cli/init/" },
      { title: "generate", href: "/docs/cli/generate/" },
      { title: "validate", href: "/docs/cli/validate/" },
      { title: "build", href: "/docs/cli/build/" },
      { title: "dev", href: "/docs/cli/dev/" },
      { title: "migrate", href: "/docs/cli/migrate/" },
      { title: "compact", href: "/docs/cli/compact/" },
      { title: "backup", href: "/docs/cli/backup/" },
      { title: "tenant", href: "/docs/cli/tenant/" },
      { title: "coordinate", href: "/docs/cli/coordinate/" },
    ],
  },
  {
    title: "Features",
    items: [
      { title: "Durability & crash safety", href: "/docs/features/durability/" },
      { title: "Transactions & MVCC", href: "/docs/features/transactions-mvcc/" },
      { title: "Indexes", href: "/docs/features/indexes/" },
      { title: "Live queries & change feed", href: "/docs/features/live-queries/" },
      { title: "Multi-tenancy & auth", href: "/docs/features/multi-tenancy/" },
      { title: "Backup & restore", href: "/docs/features/backup-restore/" },
      { title: "Point-in-time reads", href: "/docs/features/snapshot-reads/" },
      { title: "Browser read-replica", href: "/docs/features/browser-replica/" },
      { title: "Migrations", href: "/docs/features/migrations/" },
      { title: "Column projections", href: "/docs/features/projections/" },
    ],
  },
  {
    title: "Configuration",
    items: [
      { title: "forgedb.toml overview", href: "/docs/config/overview/" },
      { title: "[runtime]", href: "/docs/config/runtime/" },
      { title: "[storage]", href: "/docs/config/storage/" },
      { title: "[tenant] & [auth]", href: "/docs/config/tenant-auth/" },
      { title: "[generate] & [project]", href: "/docs/config/generate/" },
      { title: "[placement]", href: "/docs/config/placement/" },
    ],
  },
  {
    title: "Reference",
    items: [
      { title: "Generated REST API", href: "/docs/reference/rest-api/" },
      { title: "Client SDKs", href: "/docs/reference/typescript-sdk/" },
      { title: "Editor support", href: "/docs/reference/editor-support/" },
      { title: "Substrate crates", href: "/docs/reference/substrate-crates/" },
      { title: "Deployment", href: "/docs/reference/deployment/" },
      { title: "Versioning & stability", href: "/docs/reference/semver/" },
    ],
  },
];
export const flatDocs: NavItem[] = docsNav.flatMap((g) => g.items);
export function docMeta(href: string) {
  const normalized = href.endsWith("/") ? href : `${href}/`;
  const idx = flatDocs.findIndex((i) => i.href === normalized);
  const group = docsNav.find((g) => g.items.some((i) => i.href === normalized));
  return {
    group: group?.title,
    item: idx >= 0 ? flatDocs[idx] : undefined,
    prev: idx > 0 ? flatDocs[idx - 1] : undefined,
    next: idx >= 0 && idx < flatDocs.length - 1 ? flatDocs[idx + 1] : undefined,
  };
}
