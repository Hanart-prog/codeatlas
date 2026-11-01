import type { Theme } from "@/app/theme";
import type { BacktestResult } from "@/types/generated/BacktestResult";

import { interval, metricsAt, percent, RANKER_LABELS } from "./labels";

const WIDTH = 640;
const ROW = 30;
const LEFT = 150;
const RIGHT = 64;
const AXIS = 24;

const COLORS = { dark: "#3987e5", light: "#2a78d6" } as const;

interface Props {
  result: BacktestResult;
  k: number;
  theme: Theme;
}

/** Hit rate per ranker with its interval, on one shared 0 to 100% axis. */
export function IntervalChart({ result, k, theme }: Props) {
  const rows = result.rankers.flatMap((entry) => {
    const hit = metricsAt(result, entry.ranker, k)?.hit_rate;
    return hit ? [{ ranker: entry.ranker, hit }] : [];
  });
  const height = rows.length * ROW + AXIS;
  const inner = WIDTH - LEFT - RIGHT;
  const x = (value: number) => LEFT + value * inner;

  return (
    <svg
      viewBox={`0 0 ${WIDTH} ${height}`}
      role="img"
      aria-label={`Hit rate at ${k} files for each ranker. The table below has the same values.`}
      className="w-full"
    >
      {[0, 0.25, 0.5, 0.75, 1].map((tick) => (
        <g key={tick}>
          <line x1={x(tick)} x2={x(tick)} y1={0} y2={height - AXIS} stroke="var(--border)" strokeWidth={1} />
          <text x={x(tick)} y={height - 8} textAnchor="middle" fontSize={11} fill="var(--muted-foreground)">
            {tick * 100}%
          </text>
        </g>
      ))}
      {rows.map(({ ranker, hit }, index) => {
        const y = index * ROW + ROW / 2;
        const main = ranker === "combined";
        const color = main ? COLORS[theme] : "var(--muted-foreground)";
        return (
          <g key={ranker}>
            <title>{`${RANKER_LABELS[ranker]}: ${percent(hit.mean)}, ${interval(hit)}`}</title>
            <text x={LEFT - 10} y={y + 4} textAnchor="end" fontSize={12} fill="var(--foreground)">
              {RANKER_LABELS[ranker]}
            </text>
            <line x1={x(hit.low)} x2={x(hit.high)} y1={y} y2={y} stroke={color} strokeWidth={2} strokeLinecap="round" />
            <circle cx={x(hit.mean)} cy={y} r={main ? 5 : 4} fill={color} stroke="var(--card)" strokeWidth={2} />
            <text x={WIDTH - 4} y={y + 4} textAnchor="end" fontSize={12} fill="var(--foreground)">
              {percent(hit.mean)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
