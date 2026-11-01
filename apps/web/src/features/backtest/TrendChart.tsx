import { useState } from "react";

import type { Theme } from "@/app/theme";
import type { TrendBucket } from "@/types/generated/TrendBucket";

import { percent, RANKER_LABELS } from "./labels";

const WIDTH = 720;
const HEIGHT = 170;
const PAD = { top: 12, right: 12, bottom: 22, left: 44 };

/** Slots 1 and 3 of the validated palette. Both series are direct-labelled in the
 * legend and table, and the baseline is dashed, so identity is never color alone. */
const SERIES = {
  dark: { combined: "#3987e5", most_changed: "#199e70" },
  light: { combined: "#2a78d6", most_changed: "#1baf7a" },
} as const;

const RANKERS = ["combined", "most_changed"] as const;

interface Props {
  trend: TrendBucket[];
  k: number;
  theme: Theme;
}

/** Whether a result holds through history or comes from one stretch of it. */
export function TrendChart({ trend, k, theme }: Props) {
  const [active, setActive] = useState<number>();
  const innerWidth = WIDTH - PAD.left - PAD.right;
  const innerHeight = HEIGHT - PAD.top - PAD.bottom;
  const x = (index: number) => PAD.left + (trend.length > 1 ? (index / (trend.length - 1)) * innerWidth : 0);
  const y = (value: number) => PAD.top + innerHeight - value * innerHeight;
  const rate = (bucket: TrendBucket, ranker: (typeof RANKERS)[number]) =>
    bucket.rates.find((entry) => entry.ranker === ranker && entry.k === k)?.hit_rate ?? 0;
  const bucket = active === undefined ? undefined : trend[active];

  function nearest(clientX: number, rect: DOMRect): number {
    const ratio = (((clientX - rect.left) / rect.width) * WIDTH - PAD.left) / innerWidth;
    return Math.min(trend.length - 1, Math.max(0, Math.round(ratio * (trend.length - 1))));
  }

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
        {RANKERS.map((ranker) => (
          <span key={ranker} className="inline-flex items-center gap-1.5">
            <svg width="20" height="4" aria-hidden>
              <line
                x1="0"
                x2="20"
                y1="2"
                y2="2"
                stroke={SERIES[theme][ranker]}
                strokeWidth={2}
                strokeDasharray={ranker === "most_changed" ? "5 3" : undefined}
              />
            </svg>
            {RANKER_LABELS[ranker]}
          </span>
        ))}
      </div>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={`Hit rate at ${k} files through history. The table below has the same values.`}
        className="w-full cursor-crosshair"
        onPointerMove={(event) => setActive(nearest(event.clientX, event.currentTarget.getBoundingClientRect()))}
        onPointerLeave={() => setActive(undefined)}
      >
        {[0, 0.5, 1].map((tick) => (
          <g key={tick}>
            <line x1={PAD.left} x2={WIDTH - PAD.right} y1={y(tick)} y2={y(tick)} stroke="var(--border)" strokeWidth={1} />
            <text x={PAD.left - 6} y={y(tick) + 4} textAnchor="end" fontSize={11} fill="var(--muted-foreground)">
              {tick * 100}%
            </text>
          </g>
        ))}
        <text x={PAD.left} y={HEIGHT - 6} fontSize={11} fill="var(--muted-foreground)">
          Earlier
        </text>
        <text x={WIDTH - PAD.right} y={HEIGHT - 6} textAnchor="end" fontSize={11} fill="var(--muted-foreground)">
          Later
        </text>
        {active !== undefined && (
          <line x1={x(active)} x2={x(active)} y1={PAD.top} y2={PAD.top + innerHeight} stroke="var(--foreground)" strokeWidth={1} />
        )}
        {RANKERS.map((ranker) => (
          <path
            key={ranker}
            d={trend.map((entry, index) => `${index === 0 ? "M" : "L"}${x(index)} ${y(rate(entry, ranker))}`).join(" ")}
            fill="none"
            stroke={SERIES[theme][ranker]}
            strokeWidth={2}
            strokeLinejoin="round"
            strokeDasharray={ranker === "most_changed" ? "5 3" : undefined}
          />
        ))}
      </svg>
      <p className="min-h-4 text-xs text-muted-foreground" aria-live="polite">
        {bucket
          ? `${bucket.case_count} cases, commits ${bucket.from_position} to ${bucket.to_position}: ${RANKERS.map(
              (ranker) => `${RANKER_LABELS[ranker]} ${percent(rate(bucket, ranker))}`,
            ).join(", ")}`
          : "Hover for each stretch of history."}
      </p>
      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Show as a table</summary>
        <table className="mt-2 w-full text-xs">
          <caption className="sr-only">Hit rate at {k} files through history</caption>
          <thead className="text-muted-foreground">
            <tr>
              <th scope="col" className="py-1 text-left font-normal">
                Commits
              </th>
              <th scope="col" className="py-1 text-right font-normal">
                Cases
              </th>
              {RANKERS.map((ranker) => (
                <th key={ranker} scope="col" className="py-1 text-right font-normal">
                  {RANKER_LABELS[ranker]}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="font-mono">
            {trend.map((entry) => (
              <tr key={entry.from_position} className="border-t">
                <th scope="row" className="py-1 text-left font-normal">
                  {entry.from_position} to {entry.to_position}
                </th>
                <td className="py-1 text-right">{entry.case_count}</td>
                {RANKERS.map((ranker) => (
                  <td key={ranker} className="py-1 text-right">
                    {percent(rate(entry, ranker))}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}
