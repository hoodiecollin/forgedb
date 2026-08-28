import { RewriteOverlay } from "@/components/dev/rewrite-overlay";

export function DevMount() {
  if (process.env.NODE_ENV !== "development") return null;
  return <RewriteOverlay />;
}
