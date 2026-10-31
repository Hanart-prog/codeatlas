import { useState } from "react";

import type { Theme } from "@/app/theme";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { TimelinePoint } from "@/types/generated/TimelinePoint";
import type { TimelineResponse } from "@/types/generated/TimelineResponse";

const WIDTH = 720;
const HEIGHT = 96;
const PAD = { top: 10, right: 12, bottom: 6, left: 40 };
const INNER_WIDTH = WIDTH - PAD.left - PAD.right;
const INNER_HEIGHT = HEIGHT - PAD.top - PAD.bottom;

/** One hue, validated against each surface. Each chart has its own axis, so the
 * three scales never share a plot. */
const SERIES = { dark: "#3987e5", light: "#2a78d6" } as const;

const METRICS = [
  { key: "file_count", label: "Source files" },
  { key: "import_count", label: "Imports" },
  { key: "cycle_count", label: "Import cycles" },
] as const;

type Metric = (typeof METRICS)[number]["key"];

function x(index: number, count: number): number {
  return PAD.left + (count > 1 ? (index / (count - 1)) * INNER_WIDTH : 0);
}

function y(value: number, top: number): number {
  return PAD.top + INNER_HEIGHT - (top === 0 ? 0 : (value / top) * INNER_HEIGHT);
}

interface ChartProps {
  label: string;
  metric: Metric;
  points: TimelinePoint[];
  active: number | undefined;
  marks: readonly [number, number];
  color: string;
  onHover: (index: number | undefined) => void;
  onPin: (index: number) => void;
}

function Chart({ label, metric, points, active, marks, color, onHover, onPin }: ChartProps) {
  const top = Math.max(...points.map((point) => point[metric]));
  const path = points.map((point, index) => `${index === 0 ? "M" : "L"}${x(index, points.length)} ${y(point[metric], top)}`).join(" ");
  const last = points.at(-1);

  function nearest(clientX: number, rect: DOMRect): number {
    const position = ((clientX - rect.left) / rect.width) * WIDTH;
    const ratio = (position - PAD.left) / INNER_WIDTH;
    return Math.min(points.length - 1, Math.max(0, Math.round(ratio * (points.length - 1))));
  }

  return (
    <div>
      <div className="flex items-baseline justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-mono">{last?.[metric]}</span>
      </div>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={`${label} by commit. The table below has the same values.`}
        className="w-full cursor-crosshair"
        onPointerMove={(event) => onHover(nearest(event.clientX, event.currentTarget.getBoundingClientRect()))}
        onPointerLeave={() => onHover(undefined)}
        onClick={(event) => onPin(nearest(event.clientX, event.currentTarget.getBoundingClientRect()))}
      >
        {[0, top].map((value) => (
          <g key={value}>
            <line
              x1={PAD.left}
              x2={WIDTH - PAD.right}
              y1={y(value, top)}
              y2={y(value, top)}
              stroke="var(--border)"
              strokeWidth={1}
            />
            <text x={PAD.left - 6} y={y(value, top) + 4} textAnchor="end" fontSize={11} fill="var(--muted-foreground)">
              {value}
            </text>
          </g>
        ))}
        {marks.map((index, order) =>
          index < 0 ? null : (
            <line
              key={order}
              x1={x(index, points.length)}
              x2={x(index, points.length)}
              y1={PAD.top}
              y2={HEIGHT - PAD.bottom}
              stroke="var(--muted-foreground)"
              strokeWidth={1}
            />
          ),
        )}
        <path d={path} fill="none" stroke={color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {active !== undefined && points[active] && (
          <>
            <line
              x1={x(active, points.length)}
              x2={x(active, points.length)}
              y1={PAD.top}
              y2={HEIGHT - PAD.bottom}
              stroke="var(--foreground)"
              strokeWidth={1}
            />
            <circle
              cx={x(active, points.length)}
              cy={y(points[active][metric], top)}
              r={4}
              fill={color}
              stroke="var(--card)"
              strokeWidth={2}
            />
          </>
        )}
      </svg>
    </div>
  );
}

interface Props {
  timeline: TimelineResponse;
  before: string | undefined;
  after: string | undefined;
  onBefore: (oid: string) => void;
  onAfter: (oid: string) => void;
  theme: Theme;
}

/** How the import graph grew, with any point selectable for comparison. */
export function TimelineCharts({ timeline, before, after, onBefore, onAfter, theme }: Props) {
  const { points } = timeline;
  const [hovered, setHovered] = useState<number>();
  const [pinned, setPinned] = useState<number>();

  const active = hovered ?? pinned ?? points.length - 1;
  const point = points[active];
  const marks = [
    points.findIndex((entry) => entry.commit.oid === before),
    points.findIndex((entry) => entry.commit.oid === after),
  ] as const;
  const skipped = points.some((entry) => entry.skipped_file_count > 0);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Over time</CardTitle>
        <span className="text-xs text-muted-foreground">
          {points.length} of {timeline.commit_count} commits
        </span>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-3">
          {METRICS.map(({ key, label }) => (
            <Chart
              key={key}
              label={label}
              metric={key}
              points={points}
              active={active}
              marks={marks}
              color={SERIES[theme]}
              onHover={setHovered}
              onPin={setPinned}
            />
          ))}
        </div>

        {point && (
          <div aria-live="polite" className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t pt-3 text-xs">
            <span className="min-w-0 flex-1 truncate" title={point.commit.subject}>
              <span className="font-mono">{point.commit.oid.slice(0, 10)}</span>{" "}
              <span className="text-muted-foreground">{point.commit.subject}</span>
            </span>
            <span className="font-mono">
              {point.file_count} files, {point.import_count} imports, {point.cycle_count} {point.cycle_count === 1 ? "cycle" : "cycles"}
            </span>
            <span className="flex gap-2">
              <Button variant="ghost" onClick={() => onBefore(point.commit.oid)}>
                Use as Before
              </Button>
              <Button variant="ghost" onClick={() => onAfter(point.commit.oid)}>
                Use as After
              </Button>
            </span>
          </div>
        )}
        {skipped && (
          <p className="text-xs text-muted-foreground">
            Some commits have skipped files, which are counted without their imports.
          </p>
        )}

        <details>
          <summary className="cursor-pointer text-xs text-muted-foreground">Show as a table</summary>
          <div className="mt-2 max-h-64 overflow-y-auto">
            <table className="w-full text-xs">
              <caption className="sr-only">Architecture by commit, oldest first</caption>
              <thead className="text-muted-foreground">
                <tr>
                  <th scope="col" className="py-1 text-left font-normal">
                    Commit
                  </th>
                  {METRICS.map(({ key, label }) => (
                    <th key={key} scope="col" className="py-1 text-right font-normal">
                      {label}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {points.map((entry, index) => (
                  <tr key={entry.commit.oid} className="border-t">
                    <th scope="row" className="max-w-72 truncate py-1 text-left font-normal">
                      <button type="button" className="hover:underline" onClick={() => setPinned(index)}>
                        <span className="font-mono">{entry.commit.oid.slice(0, 10)}</span> {entry.commit.subject}
                      </button>
                    </th>
                    {METRICS.map(({ key }) => (
                      <td key={key} className="py-1 text-right font-mono">
                        {entry[key]}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      </CardContent>
    </Card>
  );
}
