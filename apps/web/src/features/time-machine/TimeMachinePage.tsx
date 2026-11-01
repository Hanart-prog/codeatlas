import { useCallback, useMemo, useState } from "react";

import { useCommits, useCompare, useTimeline } from "@/api/queries";
import type { Theme } from "@/app/theme";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { GraphCanvas } from "@/features/explorer/GraphCanvas";
import { EDGE_LABELS } from "@/features/explorer/labels";
import type { CompareResponse } from "@/types/generated/CompareResponse";
import type { Cycle } from "@/types/generated/Cycle";
import type { CycleChange } from "@/types/generated/CycleChange";
import type { ImportChange } from "@/types/generated/ImportChange";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

import { FileDrawer } from "./FileDrawer";
import { buildOverlay } from "./overlay";
import { TimelineCharts } from "./TimelineCharts";

const COMMIT_CHOICES = 200;
const FIELD = "h-9 w-full rounded-md border bg-card px-3 text-sm";
const LIMITATION =
  "These are changes in static imports between two commits. An added or removed import says what the source declares, not what runs.";

function MetricsTable({ response }: { response: CompareResponse }) {
  const { base, target } = response;
  const rows: [string, number, number][] = [
    ["Source files", base.file_count, target.file_count],
    ["Imports between them", base.import_count, target.import_count],
    ["Import cycles", base.cycle_count, target.cycle_count],
    ["Files in cycles", base.cyclic_file_count, target.cyclic_file_count],
    ["Largest cycle", base.largest_cycle_size, target.largest_cycle_size],
    ["Files parsed", base.coverage.parsed_file_count, target.coverage.parsed_file_count],
    ["Files partly parsed", base.coverage.partial_file_count, target.coverage.partial_file_count],
    ["Files skipped", base.coverage.skipped_file_count, target.coverage.skipped_file_count],
    ["Imports unresolved", base.coverage.unresolved_import_count, target.coverage.unresolved_import_count],
  ];
  return (
    <table className="w-full text-sm">
      <caption className="sr-only">Metrics before and after</caption>
      <thead className="text-xs text-muted-foreground">
        <tr>
          <th scope="col" className="py-1 text-left font-normal">
            Metric
          </th>
          <th scope="col" className="py-1 text-right font-normal">
            Before
          </th>
          <th scope="col" className="py-1 text-right font-normal">
            After
          </th>
          <th scope="col" className="py-1 text-right font-normal">
            Change
          </th>
        </tr>
      </thead>
      <tbody className="font-mono text-xs">
        {rows.map(([label, before, after]) => (
          <tr key={label} className="border-t">
            <th scope="row" className="py-1 text-left font-sans font-normal">
              {label}
            </th>
            <td className="py-1 text-right">{before}</td>
            <td className="py-1 text-right">{after}</td>
            <td className="py-1 text-right">{after === before ? "–" : `${after > before ? "+" : ""}${after - before}`}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function PathButton({ path, onSelect }: { path: string; onSelect: (path: string) => void }) {
  return (
    <button type="button" title={path} onClick={() => onSelect(path)} className="min-w-0 truncate font-mono text-xs hover:underline">
      {path}
    </button>
  );
}

function Section({ title, shown, total, children }: { title: string; shown: number; total: number; children: React.ReactNode }) {
  if (total === 0) return null;
  return (
    <section>
      <h3 className="text-xs text-muted-foreground">
        {title} ({total}){shown < total && `, first ${shown} shown`}
      </h3>
      <ul className="mt-1 space-y-0.5">{children}</ul>
    </section>
  );
}

function ImportRows({ changes, onSelect }: { changes: ImportChange[]; onSelect: (path: string) => void }) {
  return changes.map(({ edge, uncertain }) => (
    <li key={`${edge.source}|${edge.target}|${edge.edge_type}`} className="flex items-baseline gap-1.5 text-xs">
      <PathButton path={edge.source} onSelect={onSelect} />
      <span aria-label="imports" className="text-muted-foreground">
        →
      </span>
      <PathButton path={edge.target} onSelect={onSelect} />
      <span className="shrink-0 text-muted-foreground">
        {EDGE_LABELS[edge.edge_type]}
        {uncertain && ", uncertain"}
      </span>
    </li>
  ));
}

function CycleRows({ cycles, onSelect }: { cycles: Cycle[]; onSelect: (path: string) => void }) {
  return cycles.map((cycle) => (
    <li key={cycle.members.join("|")} className="flex flex-wrap items-baseline gap-x-2 text-xs">
      {cycle.members.map((member) => (
        <PathButton key={member} path={member} onSelect={onSelect} />
      ))}
      {cycle.type_only && <span className="text-muted-foreground">type imports only</span>}
    </li>
  ));
}

function ChangedCycleRows({ changes, onSelect }: { changes: CycleChange[]; onSelect: (path: string) => void }) {
  return changes.map((change) => (
    <li key={change.after.members.join("|")} className="text-xs">
      <span className="text-muted-foreground">
        {change.before.members.length} to {change.after.members.length} files
      </span>
      {change.joined.length > 0 && (
        <span className="ml-2 inline-flex flex-wrap gap-x-2">
          <span className="text-muted-foreground">joined</span>
          {change.joined.map((path) => (
            <PathButton key={path} path={path} onSelect={onSelect} />
          ))}
        </span>
      )}
      {change.left.length > 0 && (
        <span className="ml-2 inline-flex flex-wrap gap-x-2">
          <span className="text-muted-foreground">left</span>
          {change.left.map((path) => (
            <PathButton key={path} path={path} onSelect={onSelect} />
          ))}
        </span>
      )}
    </li>
  ));
}

function Changes({ response, onSelect }: { response: CompareResponse; onSelect: (path: string) => void }) {
  const { counts } = response;
  const empty =
    counts.added_files + counts.removed_files + counts.renamed_files + counts.added_imports + counts.removed_imports +
      counts.new_cycles + counts.resolved_cycles + counts.changed_cycles ===
    0;
  if (empty) return <p className="text-sm text-muted-foreground">No differences in the import graph between these commits.</p>;
  return (
    <div className="space-y-4">
      <Section title="New cycles" shown={response.new_cycles.length} total={counts.new_cycles}>
        <CycleRows cycles={response.new_cycles} onSelect={onSelect} />
      </Section>
      <Section title="Resolved cycles" shown={response.resolved_cycles.length} total={counts.resolved_cycles}>
        <CycleRows cycles={response.resolved_cycles} onSelect={onSelect} />
      </Section>
      <Section title="Changed cycles" shown={response.changed_cycles.length} total={counts.changed_cycles}>
        <ChangedCycleRows changes={response.changed_cycles} onSelect={onSelect} />
      </Section>
      <Section title="Added imports" shown={response.added_imports.length} total={counts.added_imports}>
        <ImportRows changes={response.added_imports} onSelect={onSelect} />
      </Section>
      <Section title="Removed imports" shown={response.removed_imports.length} total={counts.removed_imports}>
        <ImportRows changes={response.removed_imports} onSelect={onSelect} />
      </Section>
      <Section title="Added files" shown={response.added_files.length} total={counts.added_files}>
        {response.added_files.map((file) => (
          <li key={file.path} className="flex">
            <PathButton path={file.path} onSelect={onSelect} />
          </li>
        ))}
      </Section>
      <Section title="Removed files" shown={response.removed_files.length} total={counts.removed_files}>
        {response.removed_files.map((file) => (
          <li key={file.path} className="flex">
            <PathButton path={file.path} onSelect={onSelect} />
          </li>
        ))}
      </Section>
      <Section title="Renamed files" shown={response.renamed_files.length} total={counts.renamed_files}>
        {response.renamed_files.map((rename) => (
          <li key={rename.from} className="flex items-baseline gap-1.5 text-xs">
            <PathButton path={rename.from} onSelect={onSelect} />
            <span aria-label="renamed to" className="text-muted-foreground">
              →
            </span>
            <PathButton path={rename.to} onSelect={onSelect} />
            {rename.evidence === "history" && <span className="shrink-0 text-muted-foreground">through history</span>}
          </li>
        ))}
      </Section>
      {counts.imports_following_renames > 0 && (
        <p className="text-xs text-muted-foreground">
          {counts.imports_following_renames} imports only followed a rename, so they are not listed as changes.
        </p>
      )}
    </div>
  );
}

function Legend() {
  return (
    <p className="flex flex-wrap gap-x-4 gap-y-1 border-t px-4 py-2 text-xs text-muted-foreground">
      <span>Thick line or square: added</span>
      <span>Dashed: removed</span>
      <span>Double ring: renamed</span>
      <span>Diamond: in a new or resolved cycle</span>
    </p>
  );
}

interface Props {
  repository: RepositoryEntry | undefined;
  theme: Theme;
}

export function TimeMachinePage({ repository, theme }: Props) {
  const commits = useCommits(repository?.id, COMMIT_CHOICES, true);
  const compare = useCompare();
  const timeline = useTimeline(repository?.id);
  const [before, setBefore] = useState<string>();
  const [after, setAfter] = useState<string>();
  const [selected, setSelected] = useState<string>();

  const response = compare.data;
  const overlay = useMemo(() => (response ? buildOverlay(response) : undefined), [response]);
  const select = useCallback((path: string | undefined) => setSelected(path), []);

  if (!repository) {
    return (
      <div className="mx-auto max-w-6xl">
        <h1 className="text-2xl font-semibold tracking-tight">Time Machine</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }
  const repositoryId = repository.id;
  // Timeline commits can be older than the picker's page of history.
  const listed = commits.data?.commits ?? [];
  const known = new Set(listed.map((entry) => entry.oid));
  const list = [
    ...listed,
    ...[...(timeline.data?.points ?? [])]
      .reverse()
      .filter((point) => !known.has(point.commit.oid))
      .map((point) => ({ oid: point.commit.oid, subject: point.commit.subject })),
  ];
  const base = before ?? list.at(-1)?.oid;
  const target = after ?? list[0]?.oid;

  function run() {
    if (!base || !target) return;
    setSelected(undefined);
    compare.mutate({ id: repositoryId, request: { base, target } });
  }

  const picker = (label: string, value: string | undefined, onChange: (oid: string) => void) => (
    <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
      {label}
      <select className={FIELD} value={value ?? ""} onChange={(event) => onChange(event.target.value)}>
        {list.map((entry) => (
          <option key={entry.oid} value={entry.oid}>
            {entry.oid.slice(0, 10)} {entry.subject}
          </option>
        ))}
      </select>
    </label>
  );

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Time Machine</h1>
        <p className="mt-1 text-sm text-muted-foreground">{LIMITATION}</p>
      </div>

      <Card>
        <CardContent className="space-y-2">
          <div className="flex flex-wrap items-end gap-3">
            {picker("Before", base, setBefore)}
            {picker("After", target, setAfter)}
            <Button onClick={run} disabled={!base || !target || compare.isPending}>
              {compare.isPending ? "Comparing…" : "Compare"}
            </Button>
          </div>
          {commits.error && (
            <p role="alert" className="text-sm text-destructive">
              {commits.error.message}
            </p>
          )}
          {compare.error && (
            <p role="alert" className="text-sm text-destructive">
              {compare.error.message}
            </p>
          )}
        </CardContent>
      </Card>

      {timeline.error && (
        <p role="alert" className="text-sm text-destructive">
          {timeline.error.message}
        </p>
      )}
      {timeline.data && timeline.data.points.length > 1 && (
        <TimelineCharts
          timeline={timeline.data}
          before={base}
          after={target}
          onBefore={setBefore}
          onAfter={setAfter}
          theme={theme}
        />
      )}

      {response && overlay && (
        <>
          {response.notes.length > 0 && (
            <ul aria-label="Comparability notes" className="space-y-1 text-sm text-muted-foreground">
              {response.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}

          <div className="grid gap-4 lg:grid-cols-[1fr_340px]">
            <Card className="min-w-0">
              <CardHeader>
                <CardTitle>What changed</CardTitle>
                <span className="text-xs text-muted-foreground">
                  {response.base.commit.oid.slice(0, 10)} to {response.target.commit.oid.slice(0, 10)}
                </span>
              </CardHeader>
              {overlay.nodes.length > 0 && (
                <>
                  <GraphCanvas
                    nodes={overlay.nodes}
                    edges={overlay.edges}
                    fileDiff={overlay.fileDiff}
                    importDiff={overlay.importDiff}
                    selected={selected}
                    onSelect={select}
                    theme={theme}
                  />
                  <Legend />
                </>
              )}
              <CardContent className="border-t pt-4">
                <Changes response={response} onSelect={select} />
              </CardContent>
            </Card>

            <div className="min-w-0 space-y-4">
              {selected !== undefined && (
                <FileDrawer path={selected} response={response} onClose={() => setSelected(undefined)} onSelect={select} />
              )}
              <Card>
                <CardHeader>
                  <CardTitle>Before and after</CardTitle>
                </CardHeader>
                <CardContent>
                  <MetricsTable response={response} />
                </CardContent>
              </Card>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
