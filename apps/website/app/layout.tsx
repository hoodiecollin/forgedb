import type { ReactNode } from "react";
import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "./providers";
import { Analytics } from "@vercel/analytics/next";
import { Space_Grotesk, JetBrains_Mono } from "next/font/google";
import { cn } from "@/lib/utils";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { CommandMenu } from "@/components/docs/search";
import { DevMount } from "./dev-mount";

// Brand typography (docs/brand): Space Grotesk for display/wordmark/body,
// JetBrains Mono for code, CLI, and labels.
const spaceGrotesk = Space_Grotesk({ subsets: ["latin"], variable: "--font-sans" });
const jetbrainsMono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata: Metadata = {
  metadataBase: new URL("https://forgedb.dev"),
  title: {
    default: "ForgeDB — the application-database generator",
    template: "%s — ForgeDB",
  },
  description:
    "ForgeDB compiles one declarative .forge schema into a tailored Rust database, a TypeScript SDK, a REST API, and an OpenAPI spec. A generator, not a runtime ORM.",
  keywords: [
    "ForgeDB",
    "database generator",
    "schema-first",
    "Rust",
    "code generation",
    "columnar storage",
    "TypeScript SDK",
  ],
  openGraph: {
    title: "ForgeDB — the application-database generator",
    description:
      "One .forge schema → a tailored Rust database, TypeScript SDK, REST API, and an OpenAPI spec.",
    url: "https://forgedb.dev",
    siteName: "ForgeDB",
    type: "website",
  },
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html
      lang="en"
      suppressHydrationWarning
      className={cn("font-sans", spaceGrotesk.variable, jetbrainsMono.variable)}
    >
      <body className="min-h-dvh bg-background text-foreground antialiased">
        <Providers>
          <div className="relative flex min-h-dvh flex-col">
            <SiteHeader />
            <div className="flex-1">{children}</div>
            <SiteFooter />
          </div>
          <CommandMenu />
          <DevMount />
        </Providers>
        {/*
          Vercel Web Analytics — TEMPORARY launch-week calibration baseline (#194).
          Runs alongside the PostHog `/relay` proxy so we can cross-check pageview
          totals and confirm the reverse proxy is capturing correctly, then it is
          removed. Inert off-Vercel / in local dev.
        */}
        <Analytics />
      </body>
    </html>
  );
}
