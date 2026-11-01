import { useHealth } from "@/api/useHealth";
import { cn } from "@/lib/utils";

export function ApiStatus() {
  const { data, isPending, isError } = useHealth();

  const [label, tone] = isPending
    ? ["Connecting", "bg-muted-foreground"]
    : isError
      ? ["API offline", "bg-destructive"]
      : data.status === "ok"
        ? ["API connected", "bg-success"]
        : ["API degraded", "bg-warning"];

  return (
    <span role="status" className="flex items-center gap-2 text-xs whitespace-nowrap text-muted-foreground">
      <span aria-hidden className={cn("size-2 rounded-full", tone)} />
      {label}
    </span>
  );
}

/** Every page depends on the API, so losing it should not look like empty results. */
export function ApiOfflineBanner() {
  const { isError } = useHealth();
  if (!isError) return null;
  return (
    <p role="alert" className="border-b bg-destructive/10 px-6 py-2 text-sm">
      The local API is not running, so results cannot load. Start it with <code className="font-mono">make dev</code> or{" "}
      <code className="font-mono">codeatlas serve</code>.
    </p>
  );
}
