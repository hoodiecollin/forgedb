import { RewriteOverlay } from "@/components/dev/rewrite-overlay";

/**
 * Mount point for local-dev-only tooling. Returns null unless NODE_ENV is
 * "development", so it renders nothing in the production static export (and the
 * route it talks to is gitignored, so it could do nothing there anyway).
 */
export function DevMount() {
  if (process.env.NODE_ENV !== "development") return null;
  return <RewriteOverlay />;
}
