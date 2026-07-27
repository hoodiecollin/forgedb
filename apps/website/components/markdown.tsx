import { Fragment, type ElementType, type ReactNode } from "react";
import { MDXRemote } from "next-mdx-remote/rsc";
import remarkGfm from "remark-gfm";
import type { MDXComponents } from "mdx/types";
import { cn } from "@/lib/utils";

/**
 * Inline highlight — the landing page's primary-colored emphasis. Content writes
 * `<Hl>generator</Hl>` in MDX instead of a hand-authored `<span>` in the layout,
 * so purely-visual emphasis lives in the content module with the rest of the copy.
 */
function Hl({ children }: { children?: ReactNode }) {
  return <span className="text-primary">{children}</span>;
}

/**
 * Component set for marketing copy: inline-safe elements plus the custom `<Hl>`.
 * Bare `<code>` is intentionally left unstyled here — the global
 * `code:not(pre code)` rule in globals.css gives it the canonical inline chip.
 */
const contentComponents: MDXComponents = {
  Hl,
  a: ({ className, ...props }) => (
    <a
      className={cn("text-primary underline-offset-4 hover:underline", className)}
      {...props}
    />
  ),
};

/**
 * Inline variant: drop the block `<p>` wrapper so the copy flows directly inside
 * the host element (a heading, button, badge, or styled paragraph).
 */
const inlineComponents: MDXComponents = {
  ...contentComponents,
  p: ({ children }) => <Fragment>{children}</Fragment>,
};

/**
 * Renders a markdown/MDX content string with the marketing component set. Copy
 * lives in the content modules (see `content/landing.ts`); the layout feeds each
 * string here so styling stays in the content (markdown + `<Hl>`) rather than in
 * hand-authored spans.
 *
 * - `inline` strips the paragraph wrapper — use it for headings, labels, buttons,
 *   and any host element that already carries its own block styling.
 * - `as` sets the wrapper element (defaults to `span` inline, `div` block); the
 *   host keeps its `className`, so existing typography is preserved verbatim.
 * - `contentKey` stamps `data-content-key`, the anchor the in-browser rewrite
 *   tool maps a rendered element back to a slot in the content module (see
 *   `lib/dev/README.md`). Emitted only under `NODE_ENV === "development"`, so it
 *   never ships in the static export.
 */
export function Markdown({
  source,
  inline = false,
  as,
  className,
  contentKey,
}: {
  source: string;
  inline?: boolean;
  as?: ElementType;
  className?: string;
  contentKey?: string;
}) {
  const Wrapper = as ?? (inline ? "span" : "div");
  // Spread the anchor in only under dev, so the prop is entirely absent from the
  // production render *and* the RSC payload (passing `undefined` still serializes
  // the key). Statically false in prod → dead-code-eliminated.
  const devAttrs =
    process.env.NODE_ENV === "development" && contentKey ? { "data-content-key": contentKey } : {};
  return (
    <Wrapper className={className} {...devAttrs}>
      <MDXRemote
        source={source}
        components={inline ? inlineComponents : contentComponents}
        options={{ mdxOptions: { remarkPlugins: [remarkGfm] } }}
      />
    </Wrapper>
  );
}
