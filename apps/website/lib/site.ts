/** Site-wide constants. One place to change brand/links. */
export const site = {
  name: "ForgeDB",
  tagline: "The application-database generator",
  description:
    "One declarative .forge schema compiles into a tailored Rust database, a TypeScript SDK, a REST API, and an OpenAPI spec. A generator, not a runtime ORM.",
  url: "https://forgedb.dev",
  github: "https://github.com/hoodiecollin/forgedb",
  crate: "https://crates.io/crates/forgedb",
} as const;

/** Top-level nav shown in the site header. */
export const headerNav: { title: string; href: string }[] = [
  { title: "Docs", href: "/docs/" },
  { title: "Schema", href: "/docs/schema/overview/" },
  { title: "CLI", href: "/docs/cli/overview/" },
  { title: "Examples", href: "/examples/" },
];
