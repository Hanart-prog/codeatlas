import { AlertTriangle } from "lucide-react";

import { useCommits } from "@/api/queries";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { CommitPage } from "@/types/generated/CommitPage";
import type { ExclusionReason } from "@/types/generated/ExclusionReason";
import type { FileChurn } from "@/types/generated/FileChurn";
import type { OverviewResponse } from "@/types/generated/OverviewResponse";

import { formatCount, formatDate, shortOid } from "./format";

const RECENT_COMMITS = 8;

const EXCLUSION_LABELS: Record<ExclusionReason, string> = {
  binary: "binary",
  oversized: "over size limit",
  symlink: "symlinks",
  submodule: "submodules",
  rule: "matched exclusion rules",
};

function Stat({ label, value, note }: { label: string; value: number; note?: string }) {
  return (
    <div className="rounded-lg border bg-card px-4 py-3 shadow-card">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-1 text-[28px] leading-tight font-semibold tracking-tight tabular-nums">{formatCount(value)}</dd>
      {note && <dd className="mt-0.5 text-xs text-muted-foreground">{note}</dd>}
    </div>
  );
}

function Warning({ children }: { children: string }) {
  return (
    <p role="alert" className="flex items-center gap-2 text-sm">
      <AlertTriangle aria-hidden className="size-4 shrink-0 text-warning" />
      {children}
    </p>
  );
}

function CoverageNotes({ overview }: { overview: OverviewResponse }) {
  const { exclusions, files } = overview;
  if (exclusions.length === 0 && files.unreadable_path_count === 0) {
    return <p className="text-sm text-muted-foreground">No file changes were left out of metrics.</p>;
  }
  return (
    <ul className="space-y-1 text-sm text-muted-foreground">
      {exclusions.map((exclusion) => (
        <li key={exclusion.reason} className="flex justify-between gap-4">
          <span>{EXCLUSION_LABELS[exclusion.reason]}</span>
          <span className="font-mono text-foreground">{formatCount(exclusion.count)}</span>
        </li>
      ))}
      {files.unreadable_path_count > 0 && (
        <li className="flex justify-between gap-4">
          <span>unreadable paths</span>
          <span className="font-mono text-foreground">{formatCount(files.unreadable_path_count)}</span>
        </li>
      )}
    </ul>
  );
}

function ChurnRow({ file }: { file: FileChurn }) {
  return (
    <tr className="border-t">
      <td className="max-w-0 py-1.5 pr-4">
        <span className="block truncate font-mono text-xs" title={file.path}>
          {file.path}
        </span>
        {file.previous_paths.length > 0 && (
          <span className="block truncate text-xs text-muted-foreground">was {file.previous_paths.join(", ")}</span>
        )}
        {!file.exists_at_tip && <span className="block text-xs text-muted-foreground">deleted</span>}
      </td>
      <td className="py-1.5 pl-4 text-right font-mono text-xs">{formatCount(file.commit_count)}</td>
      <td className="py-1.5 pl-4 text-right font-mono text-xs">{formatCount(file.added_lines)}</td>
      <td className="py-1.5 pl-4 text-right font-mono text-xs">{formatCount(file.deleted_lines)}</td>
    </tr>
  );
}

function ChurnTable({ files }: { files: FileChurn[] }) {
  if (files.length === 0) {
    return <p className="text-sm text-muted-foreground">No eligible file changes in this history.</p>;
  }
  return (
    <table className="w-full table-fixed">
      <thead>
        <tr className="text-left text-xs text-muted-foreground">
          <th scope="col" className="pb-2 font-normal">File</th>
          <th scope="col" className="w-20 pb-2 text-right font-normal">Commits</th>
          <th scope="col" className="w-20 pb-2 text-right font-normal">Added</th>
          <th scope="col" className="w-20 pb-2 text-right font-normal">Deleted</th>
        </tr>
      </thead>
      <tbody>
        {files.map((file, index) => (
          <ChurnRow key={`${file.path}-${index}`} file={file} />
        ))}
      </tbody>
    </table>
  );
}

function RecentCommits({ page }: { page: CommitPage }) {
  return (
    <ol className="space-y-2">
      {page.commits.map((commit) => (
        <li key={commit.oid} className="flex items-baseline gap-3 text-sm">
          <span className="shrink-0 font-mono text-xs text-muted-foreground">{shortOid(commit.oid)}</span>
          <span className="min-w-0 flex-1 truncate" title={commit.subject}>
            {commit.subject}
          </span>
          {commit.parent_count > 1 && <span className="shrink-0 text-xs text-muted-foreground">merge</span>}
          <span className="shrink-0 font-mono text-xs text-muted-foreground">{formatDate(commit.committed_at)}</span>
        </li>
      ))}
    </ol>
  );
}

export function RepositoryOverview({ overview }: { overview: OverviewResponse }) {
  const { history, files, run, repository } = overview;
  const commits = useCommits(repository.id, RECENT_COMMITS, true);
  const span =
    history.first_commit && history.last_commit
      ? `${formatDate(history.first_commit.committed_at)} to ${formatDate(history.last_commit.committed_at)}`
      : undefined;

  return (
    <>
      {overview.head_oid !== null && overview.head_oid !== run.target_oid && (
        <Warning>{`The repository has moved on: HEAD is ${shortOid(overview.head_oid)}, but this analysis covers ${shortOid(run.target_oid)}. Analyze it again to include newer commits.`}</Warning>
      )}
      {overview.files.tip_typescript_file_count === 0 && (
        <Warning>This repository has no TS or TSX files. CodeAtlas analyzes only TypeScript, so its graph, impact and backtest pages will be empty.</Warning>
      )}
      {history.is_shallow && <Warning>Shallow clone: older history is missing, so metrics are incomplete.</Warning>}
      {history.is_truncated && (
        <Warning>{`The history limit of ${run.history_limit} commits cut off older history.`}</Warning>
      )}

      <dl className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="First-parent commits" value={history.commit_count} {...(span ? { note: span } : {})} />
        <Stat label="Merge commits" value={history.merge_commit_count} note="each counted once" />
        <Stat
          label="Files at analyzed commit"
          value={files.tip_file_count}
          note={`${formatCount(files.tip_typescript_file_count)} TS/TSX`}
        />
        <Stat label="Files changed" value={files.changed_file_count} note={`${formatCount(files.rename_count)} ${files.rename_count === 1 ? "rename" : "renames"}`} />
      </dl>

      <Card>
        <CardHeader>
          <CardTitle>Most changed files</CardTitle>
          <span className="text-xs text-muted-foreground">followed through renames</span>
        </CardHeader>
        <CardContent>
          <ChurnTable files={overview.top_churn} />
        </CardContent>
      </Card>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Recent commits</CardTitle>
          </CardHeader>
          <CardContent>{commits.data && <RecentCommits page={commits.data} />}</CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Left out of metrics</CardTitle>
          </CardHeader>
          <CardContent>
            <CoverageNotes overview={overview} />
            <p className="mt-4 border-t pt-3 font-mono text-xs text-muted-foreground">
              commit {shortOid(run.target_oid)} · algorithm v{run.algorithm_version} · config{" "}
              {run.config_hash.slice(0, 8)}
            </p>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
