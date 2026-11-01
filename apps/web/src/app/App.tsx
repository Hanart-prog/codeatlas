import { Moon, Sun } from "lucide-react";
import { lazy, Suspense } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { cn } from "@/lib/utils";

import { ApiOfflineBanner, ApiStatus } from "./ApiStatus";
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

const TimeMachinePage = lazy(() =>
  import("@/features/time-machine/TimeMachinePage").then((module) => ({ default: module.TimeMachinePage })),
);

const BacktestPage = lazy(() =>
  import("@/features/backtest/BacktestPage").then((module) => ({ default: module.BacktestPage })),
);

const BenchmarksPage = lazy(() =>
  import("@/features/benchmarks/BenchmarksPage").then((module) => ({ default: module.BenchmarksPage })),
);

const ReportsPage = lazy(() =>
  import("@/features/reports/ReportsPage").then((module) => ({ default: module.ReportsPage })),
);

export function App() {
  const route = useHashRoute();
  const { theme, toggle } = useTheme();
  const selection = useSelectedRepository();

  return (
    <div className="flex h-full border-t-[3px] [border-image:var(--accent)_1]">
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

      <aside className="flex w-14 shrink-0 flex-col border-r bg-sidebar lg:w-56">
        <div className="flex h-14 items-center gap-2.5 px-4">
          <img src="/favicon.svg" alt="" className="size-6" />
          <span className="sr-only text-[15px] font-semibold tracking-tight lg:not-sr-only">CodeAtlas</span>
        </div>
        <nav aria-label="Primary" className="flex-1 space-y-0.5 px-2">
          {ROUTES.map((item) => {
            const active = item.id === route.id;
            const Icon = item.icon;
            return (
              <a
                key={item.id}
                href={`#/${item.id}`}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex items-center justify-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors lg:justify-start",
                  active
                    ? "bg-muted font-medium text-foreground shadow-card"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <Icon aria-hidden className={cn("size-4 shrink-0", item.tint)} />
                <span className="sr-only flex-1 lg:not-sr-only">{item.label}</span>
              </a>
            );
          })}
        </nav>
        <p className="hidden p-4 text-xs leading-relaxed text-muted-foreground lg:block">
          Runs locally. Analyzed repositories are read-only and their code is never executed.
        </p>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="mx-auto flex h-14 w-full max-w-[calc(72rem+4rem)] shrink-0 items-center justify-between gap-4 px-4 md:px-8">
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
        <ApiOfflineBanner />
        {selection.repository?.path.endsWith("/demo/product") && (
          <div className="px-4 md:px-8">
            <p role="note" className="mx-auto mb-2 max-w-6xl text-xs text-muted-foreground">
              Generated sample repository. Its history was built to show how the tool works, so its results, including
              backtests, say nothing about real projects.
            </p>
          </div>
        )}
        <main id="main" tabIndex={-1} className="flex-1 overflow-y-auto px-4 pt-2 pb-12 outline-none md:px-8">
          {route.id === "explorer" ? (
            <Suspense fallback={<Skeleton aria-label="Loading explorer" className="h-[520px] w-full" />}>
              <ExplorerPage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : route.id === "impact" ? (
            <Suspense fallback={<Skeleton aria-label="Loading change impact" className="h-[520px] w-full" />}>
              <ImpactPage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : route.id === "time-machine" ? (
            <Suspense fallback={<Skeleton aria-label="Loading time machine" className="h-[520px] w-full" />}>
              <TimeMachinePage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : route.id === "backtesting" ? (
            <Suspense fallback={<Skeleton aria-label="Loading backtesting" className="h-[520px] w-full" />}>
              <BacktestPage repository={selection.repository} theme={theme} />
            </Suspense>
          ) : route.id === "benchmarks" ? (
            <Suspense fallback={<Skeleton aria-label="Loading benchmarks" className="h-48 w-full" />}>
              <BenchmarksPage />
            </Suspense>
          ) : route.id === "reports" ? (
            <Suspense fallback={<Skeleton aria-label="Loading reports" className="h-48 w-full" />}>
              <ReportsPage repository={selection.repository} />
            </Suspense>
          ) : (
            <OverviewPage selection={selection} />
          )}
        </main>
      </div>
    </div>
  );
}
