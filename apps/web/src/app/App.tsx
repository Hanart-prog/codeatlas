import { Moon, Sun } from "lucide-react";
import { lazy, Suspense } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { PlannedPage } from "@/features/planned/PlannedPage";
import { cn } from "@/lib/utils";

import { ApiStatus } from "./ApiStatus";
import { ROUTES } from "./routes";
import { useTheme } from "./theme";
import { useHashRoute } from "./useHashRoute";
import { useSelectedRepository } from "./useSelectedRepository";

const ExplorerPage = lazy(() =>
  import("@/features/explorer/ExplorerPage").then((module) => ({ default: module.ExplorerPage })),
);

const ImpactPage = lazy(() =>
  import("@/features/impact/ImpactPage").then((module) => ({ default: module.ImpactPage })),
);

export function App() {
  const route = useHashRoute();
  const { theme, toggle } = useTheme();
  const selection = useSelectedRepository();

  return (
    <div className="flex h-full">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-10 focus:rounded focus:bg-card focus:px-3 focus:py-1 focus:text-sm"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        Skip to content
      </a>

      <aside className="flex w-60 shrink-0 flex-col border-r bg-sidebar">
        <div className="flex h-12 items-center gap-2.5 border-b px-4">
          <img src="/favicon.svg" alt="" className="size-5" />
          <span className="text-sm font-semibold tracking-tight">CodeAtlas</span>
        </div>
        <nav aria-label="Primary" className="flex-1 space-y-0.5 p-2">
          {ROUTES.map((item) => {
            const active = item.id === route.id;
            const Icon = item.icon;
            return (
              <a
                key={item.id}
                href={`#/${item.id}`}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors",
                  active
                    ? "bg-muted font-medium text-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <Icon aria-hidden className="size-4 shrink-0" />
                <span className="flex-1">{item.label}</span>
                {item.plannedPhase !== undefined && (
                  <span className="font-mono text-[10px] text-muted-foreground">P{item.plannedPhase}</span>
                )}
              </a>
            );
          })}
        </nav>
        <p className="border-t p-4 text-xs leading-relaxed text-muted-foreground">
          Runs locally. Analyzed repositories are read-only and their code is never executed.
        </p>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b px-6">
          <span className="min-w-0 truncate font-mono text-xs text-muted-foreground">
            {selection.repository?.path ?? "No repository open"}
          </span>
          <div className="flex items-center gap-3">
            <ApiStatus />
            <Button
              variant="ghost"
              size="icon"
              aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
              onClick={toggle}
            >
              {theme === "dark" ? <Sun aria-hidden className="size-4" /> : <Moon aria-hidden className="size-4" />}
            </Button>
          </div>
        </header>
        <main id="main" tabIndex={-1} className="flex-1 overflow-y-auto p-6 outline-none">
          {route.plannedPhase !== undefined ? (
            <PlannedPage route={route} />
          ) : route.id === "explorer" ? (
            <Suspense fallback={<Skeleton aria-label="Loading explorer" className="h-[520px] w-full" />}>
              <ExplorerPage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : route.id === "impact" ? (
            <Suspense fallback={<Skeleton aria-label="Loading change impact" className="h-[520px] w-full" />}>
              <ImpactPage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : (
            <OverviewPage selection={selection} />
          )}
        </main>
      </div>
    </div>
  );
}
