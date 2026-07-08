import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

/**
 * The inspector ships as a Tauri desktop app: Next builds a fully static
 * client bundle (`output: "export"` → ./out) that Tauri serves from the
 * webview. No Node server at runtime — all data flows over Tauri IPC (at-rest
 * lens) or `fetch` to a running generated API (live lens).
 */
const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  devIndicators: false,
  // Pin the workspace root to this app — the repo root also carries a bun.lock
  // (unrelated tooling), which Next would otherwise infer as the root.
  turbopack: { root: fileURLToPath(new URL(".", import.meta.url)) },
};

export default nextConfig;
