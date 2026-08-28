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

const POSTHOG_KEY = process.env.NEXT_PUBLIC_POSTHOG_KEY;

if (typeof window !== "undefined" && POSTHOG_KEY && !posthog.__loaded) {
  posthog.init(POSTHOG_KEY, {
    api_host: "/relay",
    ui_host: "https://us.posthog.com",
    person_profiles: "identified_only",
    capture_pageview: false,
    capture_pageleave: true,
  });
}

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
