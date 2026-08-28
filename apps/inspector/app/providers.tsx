"use client";

import type { ReactNode } from "react";
import { Provider } from "jotai";
import { ThemeProvider } from "next-themes";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";

export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="dark"
      enableSystem
      disableTransitionOnChange
    >
      <Provider>
        <TooltipProvider>{children}</TooltipProvider>
        <Toaster />
      </Provider>
    </ThemeProvider>
  );
}
