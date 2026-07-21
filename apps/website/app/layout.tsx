import type { ReactNode } from "react";
import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "./providers";
import { Geist, Geist_Mono } from "next/font/google";
import { cn } from "@/lib/utils";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { CommandMenu } from "@/components/docs/search";

const geist = Geist({ subsets: ["latin"], variable: "--font-sans" });
const geistMono = Geist_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata: Metadata = {
  metadataBase: new URL("https://forgedb.dev"),
  title: {
    default: "ForgeDB — the application-database generator",
    template: "%s — ForgeDB",
  },
  description:
    "ForgeDB compiles one declarative .forge schema into a tailored Rust database, a TypeScript SDK, a REST API, and React stubs. A generator, not a runtime ORM.",
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
      "One .forge schema → a tailored Rust database, TypeScript SDK, REST API, and React stubs.",
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
      className={cn("font-sans", geist.variable, geistMono.variable)}
    >
      <body className="min-h-dvh bg-background text-foreground antialiased">
        <Providers>
          <div className="relative flex min-h-dvh flex-col">
            <SiteHeader />
            <div className="flex-1">{children}</div>
            <SiteFooter />
          </div>
          <CommandMenu />
        </Providers>
      </body>
    </html>
  );
}
