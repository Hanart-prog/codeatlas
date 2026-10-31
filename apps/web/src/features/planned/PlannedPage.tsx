import type { AppRoute } from "@/app/routes";

export function PlannedPage({ route }: { route: AppRoute }) {
  const Icon = route.icon;
  return (
    <div className="mx-auto max-w-4xl">
      <h1 className="text-xl font-semibold tracking-tight">{route.label}</h1>
      <div className="mt-6 flex flex-col items-center rounded-lg border border-dashed px-6 py-16 text-center">
        <Icon aria-hidden className="size-6 text-muted-foreground" />
        <p className="mt-4 max-w-md text-sm leading-relaxed text-muted-foreground">{route.purpose}</p>
        <p className="mt-4 font-mono text-xs text-muted-foreground">Not built yet · planned for Phase {route.plannedPhase}</p>
      </div>
    </div>
  );
}
