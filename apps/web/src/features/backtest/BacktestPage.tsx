import { type FormEvent, useState } from "react";

import { useLatestBacktest, useRunBacktest } from "@/api/queries";
import { JobActivity } from "@/components/JobActivity";
import type { Theme } from "@/app/theme";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { BacktestResult } from "@/types/generated/BacktestResult";
import type { JobResponse } from "@/types/generated/JobResponse";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

import { IntervalChart } from "./IntervalChart";
import { interval, metricsAt, percent, RANKER_LABELS, verdict } from "./labels";
import { TrendChart } from "./TrendChart";

const FIELD = "h-9 rounded-md border bg-card px-3 text-sm";
const LIMITATION =
  "This measures co-edit prediction: whether files changed together in later commits. It does not measure bugs, real causal impact or developer productivity, and a held-out file may have changed for an unrelated reason.";

function parseCutoffs(text: string): number[] {
  return text
    .split(",")
    .map((part) => Number(part.trim()))
    .filter((value) => Number.isInteger(value) && value > 0);
}

function MetricCard({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 font-mono text-2xl">{value}</p>
      <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
    </div>
  );
}

function RankerTable({ result, k }: { result: BacktestResult; k: number }) {
  return (
    <table className="w-full text-xs">
      <caption className="sr-only">Every ranker at {k} files</caption>
      <thead className="text-muted-foreground">
        <tr>
          {["Ranker", "Hit rate", "Interval", "Precision", "Recall", "Mean candidates", "Empty cases"].map((title, index) => (
            <th key={title} scope="col" className={cn("py-1 font-normal", index === 0 ? "text-left" : "text-right")}>
              {title}
            </th>
          ))}
        </tr>
      </thead>
      <tbody className="font-mono">
        {result.rankers.map((entry) => {
          const metrics = metricsAt(result, entry.ranker, k);
          if (!metrics) return null;
          return (
            <tr key={entry.ranker} className="border-t">
              <th scope="row" className="py-1 text-left font-sans font-normal">
                {RANKER_LABELS[entry.ranker]}
              </th>
              <td className="py-1 text-right">{percent(metrics.hit_rate.mean)}</td>
              <td className="py-1 text-right">{interval(metrics.hit_rate)}</td>
              <td className="py-1 text-right">{percent(metrics.precision.mean)}</td>
              <td className="py-1 text-right">{percent(metrics.recall.mean)}</td>
              <td className="py-1 text-right">{entry.mean_candidate_count.toFixed(1)}</td>
              <td className="py-1 text-right">{entry.empty_case_count}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function Counts({ result }: { result: BacktestResult }) {
  const { counts } = result;
  const rows: [string, number][] = [
    ["First-parent commits", counts.commit_count],
    ["No TS or TSX files", counts.no_eligible_file_commits],
    ["Over the bulk limit", counts.bulk_commits],
    ["Used as warm-up", counts.warmup_commits],
    ["Candidate cases", counts.candidate_cases],
    ["Skipped: one eligible file", counts.too_few_files],
    ["Skipped: no input at parent", counts.no_input_at_parent],
    ["Skipped: nothing to predict", counts.no_target_at_parent],
    ["Evaluable cases", counts.evaluable_cases],
    ["Evaluated cases", counts.evaluated_cases],
    ["Merge commits among them", counts.merge_cases],
    ["Held-out files", counts.target_files],
    ["New files left out", counts.new_files_excluded],
    ["Renamed targets", counts.renamed_targets],
    ["Deleted targets", counts.deleted_targets],
  ];
  return (
    <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 text-xs">
      {rows.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="text-right font-mono">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Configuration({ result }: { result: BacktestResult }) {
  const rows: [string, string][] = [
    ["Commit", result.target_oid.slice(0, 10)],
    ["Warm-up", String(result.warmup)],
    ["Case limit", String(result.max_cases)],
    ["Cutoffs", result.top_k.join(", ")],
    ["History window", String(result.history_window)],
    ["Seed", String(result.seed)],
    ["Versions", `parser ${result.parser_version}, analysis ${result.algorithm_version}, backtest ${result.backtest_version}`],
    ["Settings hash", result.config_hash.slice(0, 12)],
  ];
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
      {rows.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="text-right font-mono">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Cases({ result, k }: { result: BacktestResult; k: number }) {
  return (
    <details>
      <summary className="cursor-pointer text-sm">Every evaluated case ({result.cases.length})</summary>
      <div className="mt-2 max-h-96 overflow-y-auto">
        <table className="w-full text-xs">
          <caption className="sr-only">Evaluated commits and their Change Impact hits at {k} files</caption>
          <thead className="text-muted-foreground">
            <tr>
              <th scope="col" className="py-1 text-left font-normal">
                Commit
              </th>
              <th scope="col" className="py-1 text-left font-normal">
                Input
              </th>
              <th scope="col" className="py-1 text-right font-normal">
                Hits of held-out
              </th>
            </tr>
          </thead>
          <tbody>
            {result.cases.map((entry) => {
              const ranking = entry.rankings.find((item) => item.ranker === "combined");
              const top = new Set(ranking?.paths.slice(0, k));
              const hits = entry.targets.filter((path) => top.has(path)).length;
              return (
                <tr key={entry.commit.oid} className="border-t">
                  <th scope="row" className="max-w-80 truncate py-1 text-left font-normal" title={entry.commit.subject}>
                    <span className="font-mono">{entry.commit.oid.slice(0, 10)}</span> {entry.commit.subject}
                  </th>
                  <td className="max-w-64 truncate py-1 font-mono" title={entry.input}>
                    {entry.input}
                  </td>
                  <td className="py-1 text-right font-mono">
                    {hits} of {entry.targets.length}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </details>
  );
}

function exportJson(result: BacktestResult) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `codeatlas-backtest-${result.target_oid.slice(0, 10)}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function Results({ result, theme }: { result: BacktestResult; theme: Theme }) {
  const [chosen, setChosen] = useState<number>();
  const k = chosen !== undefined && result.top_k.includes(chosen) ? chosen : (result.top_k[0] ?? 1);
  const metrics = metricsAt(result, "combined", k);
  const sensitivity = result.seeded_input.find((entry) => entry.k === k);

  return (
    <div className="space-y-4">
      {result.notes.length > 0 && (
        <ul aria-label="Limits of this run" className="space-y-1 text-sm text-muted-foreground">
          {result.notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
      {result.counts.evaluated_cases === 0 || !metrics ? null : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm font-medium">{verdict(result, k)}</p>
            <div className="flex items-center gap-3">
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                Files inspected
                <select className={FIELD} value={k} onChange={(event) => setChosen(Number(event.target.value))}>
                  {result.top_k.map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
                </select>
              </label>
              <Button variant="ghost" onClick={() => exportJson(result)}>
                Export JSON
              </Button>
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <MetricCard label={`Hit rate at ${k}`} value={percent(metrics.hit_rate.mean)} detail={interval(metrics.hit_rate)} />
            <MetricCard label={`Recall at ${k}`} value={percent(metrics.recall.mean)} detail={interval(metrics.recall)} />
            <MetricCard label={`Precision at ${k}`} value={percent(metrics.precision.mean)} detail={interval(metrics.precision)} />
            <MetricCard
              label="Coverage"
              value={percent(result.coverage)}
              detail={`${result.counts.evaluable_cases} of ${result.counts.candidate_cases} candidate cases`}
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
            <div className="min-w-0 space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle>Against the baselines</CardTitle>
                  <span className="text-xs text-muted-foreground">Dot: mean. Line: bootstrap 95% interval.</span>
                </CardHeader>
                <CardContent className="space-y-3">
                  <IntervalChart result={result} k={k} theme={theme} />
                  <RankerTable result={result} k={k} />
                  {sensitivity && (
                    <p className="text-xs text-muted-foreground">
                      With a seeded input file in place of the smallest path, Change Impact hits{" "}
                      {percent(sensitivity.hit_rate.mean)} ({interval(sensitivity.hit_rate)}).
                    </p>
                  )}
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle>Through history</CardTitle>
                </CardHeader>
                <CardContent>
                  <TrendChart trend={result.trend} k={k} theme={theme} />
                </CardContent>
              </Card>
              <Cases result={result} k={k} />
            </div>
            <div className="min-w-0 space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle>Evaluation counts</CardTitle>
                </CardHeader>
                <CardContent>
                  <Counts result={result} />
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle>Configuration</CardTitle>
                </CardHeader>
                <CardContent>
                  <Configuration result={result} />
                </CardContent>
              </Card>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

interface Props {
  repository: RepositoryEntry | undefined;
  theme: Theme;
}

export function BacktestPage({ repository, theme }: Props) {
  const latest = useLatestBacktest(repository?.id);
  const [job, setJob] = useState<JobResponse>();
  const run = useRunBacktest(setJob);
  const [warmup, setWarmup] = useState(100);
  const [maxCases, setMaxCases] = useState(200);
  const [cutoffs, setCutoffs] = useState("5, 10");

  if (!repository) {
    return (
      <div className="mx-auto max-w-6xl">
        <h1 className="text-2xl font-semibold tracking-tight">Backtesting</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }
  const repositoryId = repository.id;

  function submit(event: FormEvent) {
    event.preventDefault();
    setJob(undefined);
    run.mutate({
      id: repositoryId,
      request: { warmup, max_cases: maxCases, top_k: parseCutoffs(cutoffs) },
    });
  }

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Backtesting</h1>
        <p className="mt-1 text-sm text-muted-foreground">{LIMITATION}</p>
      </div>

      <Card>
        <CardContent className="space-y-2">
          <form onSubmit={submit} className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Warm-up commits
              <input
                type="number"
                min={1}
                className={cn(FIELD, "w-28")}
                value={warmup}
                onChange={(event) => setWarmup(Number(event.target.value))}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Most cases
              <input
                type="number"
                min={1}
                max={1000}
                className={cn(FIELD, "w-28")}
                value={maxCases}
                onChange={(event) => setMaxCases(Number(event.target.value))}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Files inspected
              <input className={cn(FIELD, "w-28")} value={cutoffs} onChange={(event) => setCutoffs(event.target.value)} />
            </label>
            <Button type="submit" disabled={run.isPending}>
              {run.isPending ? "Running…" : "Run backtest"}
            </Button>
            {run.isPending && job && <JobActivity key={job.id} job={job} />}
          </form>
          {(run.error ?? latest.error) && (
            <p role="alert" className="text-sm text-destructive">
              {(run.error ?? latest.error)?.message}
            </p>
          )}
        </CardContent>
      </Card>

      {latest.isLoading && <Skeleton aria-label="Loading backtest" className="h-64 w-full" />}
      {latest.data?.entry === null && !run.isPending && (
        <p className="text-sm text-muted-foreground">No backtest has run for this repository yet.</p>
      )}
      {latest.data?.entry && <Results key={latest.data.entry.created_at} result={latest.data.entry.result} theme={theme} />}
    </div>
  );
}
