"use client";

import type { ReactNode } from "react";
import { Suspense, useEffect } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import { Provider } from "jotai";
import { ThemeProvider } from "next-themes";
import posthog from "posthog-js";
import { PostHogProvider, usePostHog } from "posthog-js/react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";

// Publishable PostHog project key (client-safe, ships in the bundle). Absent in
// local dev — the tree stays inert if so; the real value lives in the Vercel
// project env for production/preview builds.
const POSTHOG_KEY = process.env.NEXT_PUBLIC_POSTHOG_KEY;

// Initialise once on the client. Events go to the first-party `/relay` path,
// which `vercel.json` reverse-proxies to PostHog US — keeping ingestion
// same-origin so ad-blocker filter lists (which match *.posthog.com) don't fire.
if (typeof window !== "undefined" && POSTHOG_KEY && !posthog.__loaded) {
  posthog.init(POSTHOG_KEY, {
    api_host: "/relay",
    ui_host: "https://us.posthog.com",
    person_profiles: "identified_only",
    capture_pageview: false, // captured manually below (app-router SPA nav)
    capture_pageleave: true,
  });
}

/**
 * App-router client navigations don't trigger a full page load, so PostHog's
 * automatic pageview capture would miss them. Capture `$pageview` on every
 * path/query change instead. Reads `useSearchParams`, so it must live inside a
 * Suspense boundary.
 */
function PostHogPageView() {
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const ph = usePostHog();

  useEffect(() => {
    if (!pathname || !ph) return;
    let url = window.origin + pathname;
    const qs = searchParams?.toString();
    if (qs) url += `?${qs}`;
    ph.capture("$pageview", { $current_url: url });
  }, [pathname, searchParams, ph]);

  return null;
}

/**
 * Client providers: theme (dark by default, follows system preference) + jotai
 * store + shadcn tooltip context + toast portal. Mirrors the inspector app so
 * the two share one visual language. When a PostHog key is configured, the tree
 * is additionally wrapped in the PostHog provider + manual pageview tracker.
 */
export function Providers({ children }: { children: ReactNode }) {
  const tree = (
    <ThemeProvider
      attribute="class"
      defaultTheme="dark"
      enableSystem
      disableTransitionOnChange
    >
      <Provider>
        <TooltipProvider delayDuration={200}>{children}</TooltipProvider>
        <Toaster />
      </Provider>
    </ThemeProvider>
  );

  // No key (e.g. local dev): render the app untouched — analytics stays inert.
  if (!POSTHOG_KEY) return tree;

  return (
    <PostHogProvider client={posthog}>
      <Suspense fallback={null}>
        <PostHogPageView />
      </Suspense>
      {tree}
    </PostHogProvider>
  );
}
