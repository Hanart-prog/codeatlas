import type { ComponentProps } from "react";

import { cn } from "@/lib/utils";

export function Card({ className, ...props }: ComponentProps<"section">) {
  return <section className={cn("rounded-lg border bg-card", className)} {...props} />;
}

export function CardHeader({ className, ...props }: ComponentProps<"div">) {
  return (
    <div className={cn("flex items-center justify-between gap-4 border-b px-4 py-3", className)} {...props} />
  );
}

export function CardTitle({ className, ...props }: ComponentProps<"h2">) {
  return <h2 className={cn("text-sm font-medium", className)} {...props} />;
}

export function CardContent({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("p-4", className)} {...props} />;
}
