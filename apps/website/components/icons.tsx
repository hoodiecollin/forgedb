import type { SVGProps } from "react";
import { cn } from "@/lib/utils";

/**
 * ForgeDB logomark (docs/brand `forgemark-primary.svg`) — a schema "F" node
 * fanning into streams; the top stream runs molten (what's being forged now),
 * the rest violet. Colors are fixed brand hex, not `currentColor`.
 */
export function ForgeMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 120 120" fill="none" aria-hidden {...props}>
      <mask id="forgemark-f">
        <rect x="8" y="38" width="44" height="44" rx="11" fill="#fff" />
        <rect x="20" y="47" width="7.5" height="26" rx="1.5" fill="#000" />
        <rect x="20" y="47" width="21" height="7.5" rx="1.5" fill="#000" />
        <rect x="20" y="58" width="15" height="7.5" rx="1.5" fill="#000" />
      </mask>
      <path d="M50 60 C68 60 74 33 94 32" stroke="#f59e0b" strokeWidth="4.5" strokeLinecap="round" />
      <path d="M50 60 C68 60 78 50 94 50" stroke="#a684ff" strokeWidth="4.5" strokeLinecap="round" />
      <path d="M50 60 C68 60 78 70 94 70" stroke="#a684ff" strokeWidth="4.5" strokeLinecap="round" />
      <path d="M50 60 C68 60 74 87 94 88" stroke="#a684ff" strokeWidth="4.5" strokeLinecap="round" />
      <circle cx="98" cy="32" r="5.5" fill="#f59e0b" />
      <circle cx="98" cy="50" r="5.5" fill="#a684ff" />
      <circle cx="98" cy="70" r="5.5" fill="#a684ff" />
      <circle cx="98" cy="88" r="5.5" fill="#a684ff" />
      <rect x="8" y="38" width="44" height="44" rx="11" fill="#a684ff" mask="url(#forgemark-f)" />
    </svg>
  );
}

/**
 * ForgeDB wordmark — two-tone "Forge" (foreground) + "DB" (violet primary),
 * set in the brand display face (Space Grotesk, via `--font-sans`).
 */
export function ForgeWordmark({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span className={cn("font-semibold tracking-tight", className)} {...props}>
      Forge<span className="text-primary">DB</span>
    </span>
  );
}

export function GitHubIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden {...props}>
      <path d="M12 .5C5.37.5 0 5.87 0 12.5c0 5.3 3.44 9.8 8.21 11.39.6.11.82-.26.82-.58 0-.29-.01-1.04-.02-2.05-3.34.73-4.04-1.61-4.04-1.61-.55-1.39-1.33-1.76-1.33-1.76-1.09-.74.08-.73.08-.73 1.2.09 1.84 1.24 1.84 1.24 1.07 1.83 2.81 1.3 3.5.99.11-.78.42-1.3.76-1.6-2.67-.3-5.47-1.33-5.47-5.93 0-1.31.47-2.38 1.24-3.22-.12-.3-.54-1.52.12-3.18 0 0 1.01-.32 3.3 1.23a11.5 11.5 0 0 1 6.01 0c2.29-1.55 3.3-1.23 3.3-1.23.66 1.66.24 2.88.12 3.18.77.84 1.23 1.91 1.23 3.22 0 4.61-2.8 5.62-5.48 5.92.43.37.81 1.1.81 2.22 0 1.6-.01 2.9-.01 3.29 0 .32.21.7.82.58A12.01 12.01 0 0 0 24 12.5C24 5.87 18.63.5 12 .5Z" />
    </svg>
  );
}
