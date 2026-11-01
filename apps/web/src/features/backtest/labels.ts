import type { BacktestResult } from "@/types/generated/BacktestResult";
import type { Estimate } from "@/types/generated/Estimate";
import type { MetricsAtK } from "@/types/generated/MetricsAtK";
import type { Ranker } from "@/types/generated/Ranker";

export const RANKER_LABELS: Record<Ranker, string> = {
  combined: "Change Impact",
  co_change: "Co-change only",
  reverse_dependency: "Imports only",
  most_changed: "Most changed files",
  random: "Random",
};

export function percent(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

export function interval(estimate: Estimate): string {
  return `${(estimate.low * 100).toFixed(0)} to ${(estimate.high * 100).toFixed(0)}%`;
}

export function metricsAt(result: BacktestResult, ranker: Ranker, k: number): MetricsAtK | undefined {
  return result.rankers.find((entry) => entry.ranker === ranker)?.metrics.find((metrics) => metrics.k === k);
}

/** A plain reading of Change Impact against the strongest real baseline. */
export function verdict(result: BacktestResult, k: number): string | undefined {
  const impact = metricsAt(result, "combined", k)?.hit_rate;
  const best = (["most_changed", "co_change", "reverse_dependency"] as const)
    .map((ranker) => ({ ranker, hit: metricsAt(result, ranker, k)?.hit_rate }))
    .filter((entry): entry is { ranker: (typeof entry)["ranker"]; hit: Estimate } => entry.hit !== undefined)
    .sort((a, b) => b.hit.mean - a.hit.mean)[0];
  if (!impact || !best) return undefined;

  const name = RANKER_LABELS[best.ranker];
  const values = `Change Impact ${percent(impact.mean)}, ${name} ${percent(best.hit.mean)}`;
  if (impact.high < best.hit.low) return `${name} did better than Change Impact at ${k} files (${values}).`;
  if (impact.low > best.hit.high) return `Change Impact did better than ${name} at ${k} files (${values}).`;
  return `This run cannot separate Change Impact from ${name} at ${k} files: their intervals overlap (${values}).`;
}
