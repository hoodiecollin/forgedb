import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

/**
 * The marketing + docs site builds to a fully static export (`output: "export"`
 * → ./out) so it can be hosted anywhere (Cloudflare Pages, GitHub Pages, Vercel,
 * an S3 bucket) with no Node server at runtime. All content is compiled from MDX
 * at build time; search runs client-side over a prebuilt static index.
 */
const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  devIndicators: false,
  // Trailing slashes keep the static export's directory-per-route URLs stable
  // across hosts (`/docs/foo/` → `docs/foo/index.html`).
  trailingSlash: true,
  // Pin the workspace root to this app — the repo root also carries a bun.lock
  // (unrelated tooling), which Next would otherwise infer as the root.
  turbopack: { root: fileURLToPath(new URL(".", import.meta.url)) },
};

export default nextConfig;
