import { Fragment, type ElementType, type ReactNode } from "react";
import { MDXRemote } from "next-mdx-remote/rsc";
import remarkGfm from "remark-gfm";
import type { MDXComponents } from "mdx/types";
import { cn } from "@/lib/utils";

function Hl({ children }: { children?: ReactNode }) {
  return <span className="text-primary">{children}</span>;
}
const contentComponents: MDXComponents = {
  Hl,
  a: ({ className, ...props }) => (
    <a
      className={cn("text-primary underline-offset-4 hover:underline", className)}
      {...props}
    />
  ),
};
const inlineComponents: MDXComponents = {
  ...contentComponents,
  p: ({ children }) => <Fragment>{children}</Fragment>,
};
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
