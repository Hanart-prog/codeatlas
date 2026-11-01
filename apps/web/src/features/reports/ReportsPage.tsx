import { ApiRequestError, reportUrl } from "@/api/client";
import { useReport } from "@/api/queries";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import type { ReportDocument } from "@/types/generated/ReportDocument";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

const LINK = "inline-flex h-9 items-center rounded-md border bg-card px-4 text-sm font-medium hover:bg-muted";
const MAIN_LINK = "inline-flex h-9 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90";

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1">
      <dt className="shrink-0 text-sm text-muted-foreground">{label}</dt>
      <dd className="min-w-0 truncate font-mono text-xs" title={value}>
        {value}
      </dd>
    </div>
  );
}

function Contents({ report }: { report: ReportDocument }) {
  const { architecture, backtest, history, provenance, timeline, top_churn: churn } = report;
  const rows = [
    `History: ${history.commit_count.toLocaleString("en-US")} first-parent commits, ${churn.length} most changed files`,
    architecture
      ? `Import graph: ${architecture.file_count.toLocaleString("en-US")} files, ${architecture.import_count.toLocaleString("en-US")} imports, ${architecture.cycle_count} cycles`
      : "Import graph: left out, because the repository is over a graph limit",
    `Timeline: ${timeline.length} commits`,
    backtest
      ? `Backtest: ${backtest.counts.evaluated_cases} evaluated commits against the baselines`
      : "Backtest: left out, because none has run on this commit. Run one from Backtesting to include it.",
  ];
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle>What it holds</CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="space-y-1.5 text-sm">
            {rows.map((row) => (
              <li key={row}>{row}</li>
            ))}
          </ul>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Provenance</CardTitle>
        </CardHeader>
        <CardContent>
          <dl>
            <Row label="Repository" value={provenance.repository_name} />
            <Row label="Commit" value={provenance.commit} />
            <Row label="CodeAtlas" value={provenance.tool_version} />
            <Row label="Parser version" value={String(provenance.parser_version)} />
            <Row label="Algorithm version" value={String(provenance.algorithm_version)} />
            <Row label="Backtest version" value={String(provenance.backtest_version)} />
            <Row label="Settings hash" value={provenance.config_hash} />
          </dl>
        </CardContent>
      </Card>
    </div>
  );
}

export function ReportsPage({ repository }: { repository: RepositoryEntry | undefined }) {
  const report = useReport(repository?.id);

  if (!repository) {
    return (
      <div className="mx-auto max-w-6xl">
        <h1 className="text-2xl font-semibold tracking-tight">Reports</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }
  const notAnalyzed = report.error instanceof ApiRequestError && report.error.code === "not_found";

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Reports</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          One file to keep or share. The HTML page runs no script and loads nothing, and neither format holds your local
          path.
        </p>
      </div>
      {notAnalyzed && <p className="text-sm text-muted-foreground">Analyze this repository from the Overview first.</p>}
      {report.error && !notAnalyzed && (
        <p role="alert" className="text-sm text-destructive">
          {report.error.message}
        </p>
      )}
      {report.isLoading && <Skeleton aria-label="Loading report" className="h-48 w-full" />}
      {report.data && (
        <>
          <div className="flex gap-2">
            <a className={MAIN_LINK} href={reportUrl(repository.id, "html")} download="codeatlas-report.html">
              Download HTML
            </a>
            <a className={LINK} href={reportUrl(repository.id, "json")} download="codeatlas-report.json">
              Download JSON
            </a>
          </div>
          <Contents report={report.data} />
        </>
      )}
    </div>
  );
}
