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
    <span role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
      <span aria-hidden className={cn("size-2 rounded-full", tone)} />
      {label}
    </span>
  );
}
