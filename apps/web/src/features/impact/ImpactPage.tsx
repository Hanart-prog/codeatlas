import { type FormEvent, useCallback, useId, useMemo, useState } from "react";

import { useCommits, useGraph, useImpact } from "@/api/queries";
import { useDebounced } from "@/app/useDebounced";
import type { Theme } from "@/app/theme";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { GraphCanvas } from "@/features/explorer/GraphCanvas";
import { cn } from "@/lib/utils";
import type { ImpactResponse } from "@/types/generated/ImpactResponse";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

import { Recommendation } from "./Recommendation";

const COMMIT_CHOICES = 200;
const SUGGESTIONS = 30;
const FIELD = "h-9 rounded-md border bg-card px-3 text-sm";
const LIMITATION =
  "Scores rank files to inspect. A score is a heuristic, not a probability of failure, and history shows files that changed together, not that one caused the other.";

function HistoryNote({ response }: { response: ImpactResponse }) {
  const { history } = response;
  return (
    <p className="text-xs text-muted-foreground">
      As of {response.cutoff.slice(0, 10)}: {history.commit_count} first-parent commits read,{" "}
      {history.eligible_commit_count} eligible, {history.bulk_commit_count} over the {history.bulk_threshold}-file bulk
      limit, {history.no_eligible_file_commit_count} with no TS or TSX files. {response.candidate_count} files were
      ranked.
      {history.is_shallow && " Shallow clone: older history is missing."}
      {history.is_truncated && " The window cut off older history."}
    </p>
  );
}

interface Props {
  repository: RepositoryEntry | undefined;
  theme: Theme;
}

export function ImpactPage({ repository, theme }: Props) {
  const [selected, setSelected] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const [cutoff, setCutoff] = useState<string>();
  const [maxDepth, setMaxDepth] = useState(3);
  const [topK, setTopK] = useState(10);
  const listId = useId();

  const graph = useGraph(repository?.id, { ref: cutoff });
  const typed = useDebounced(draft.trim());
  const suggestions = useGraph(typed ? repository?.id : undefined, { ref: cutoff, search: typed, limit: SUGGESTIONS });
  const commits = useCommits(repository?.id, COMMIT_CHOICES, true);
  const impact = useImpact();
  const response = impact.data;
  const noop = useCallback(() => {}, []);

  const scoped = useMemo(() => {
    if (!response || !graph.data) return undefined;
    const paths = new Set([
      ...response.selected.map((file) => file.path),
      ...response.recommendations.map((item) => item.path),
    ]);
    return {
      nodes: graph.data.nodes.filter((node) => paths.has(node.path)),
      edges: graph.data.edges.filter((edge) => paths.has(edge.source) && paths.has(edge.target)),
      emphasized: response.selected.map((file) => file.path),
    };
  }, [response, graph.data]);

  if (!repository) {
    return (
      <div className="mx-auto max-w-6xl">
        <h1 className="text-2xl font-semibold tracking-tight">Change Impact</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }
  const repositoryId = repository.id;

  function addFile(event: FormEvent) {
    event.preventDefault();
    const path = draft.trim();
    if (path && !selected.includes(path)) setSelected([...selected, path]);
    setDraft("");
  }

  function run() {
    impact.mutate({
      id: repositoryId,
      request: {
        selected_paths: selected,
        max_depth: maxDepth,
        top_k: topK,
        ...(cutoff ? { cutoff } : {}),
      },
    });
  }

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Change Impact</h1>
        <p className="mt-1 text-sm text-muted-foreground">{LIMITATION}</p>
      </div>

      <Card>
        <CardContent className="space-y-3">
          <form onSubmit={addFile} className="flex items-end gap-2">
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
              Add a file
              <input
                className={cn(FIELD, "font-mono")}
                list={listId}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder="src/auth/session.ts"
                spellCheck={false}
                autoComplete="off"
              />
            </label>
            <datalist id={listId}>
              {(typed ? suggestions.data : graph.data)?.nodes.slice(0, SUGGESTIONS).map((node) => (
                <option key={node.path} value={node.path} />
              ))}
            </datalist>
            <Button type="submit" disabled={!draft.trim()}>
              Add
            </Button>
          </form>

          {selected.length > 0 && (
            <ul aria-label="Selected files" className="flex flex-wrap gap-2">
              {selected.map((path) => (
                <li key={path} className="flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 font-mono text-xs">
                  {path}
                  <button
                    type="button"
                    aria-label={`Remove ${path}`}
                    className="px-1 text-muted-foreground hover:text-foreground"
                    onClick={() => setSelected(selected.filter((item) => item !== path))}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}

          <div className="flex flex-wrap items-end gap-3">
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
              As of commit
              <select
                className={FIELD}
                value={cutoff ?? ""}
                onChange={(event) => setCutoff(event.target.value || undefined)}
              >
                <option value="">Latest analyzed commit</option>
                {commits.data?.commits.map((entry) => (
                  <option key={entry.oid} value={entry.oid}>
                    {entry.oid.slice(0, 10)} {entry.subject}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Import hops
              <input
                type="number"
                min={1}
                max={10}
                className={cn(FIELD, "w-20")}
                value={maxDepth}
                onChange={(event) => setMaxDepth(Number(event.target.value))}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Results
              <input
                type="number"
                min={1}
                max={100}
                className={cn(FIELD, "w-20")}
                value={topK}
                onChange={(event) => setTopK(Number(event.target.value))}
              />
            </label>
            <Button onClick={run} disabled={selected.length === 0 || impact.isPending}>
              {impact.isPending ? "Ranking…" : "Rank files"}
            </Button>
          </div>
          {impact.error && (
            <p role="alert" className="text-sm text-destructive">
              {impact.error.message}
            </p>
          )}
        </CardContent>
      </Card>

      {response && (
        <>
          {response.notes.length > 0 && (
            <ul className="space-y-1 text-sm text-muted-foreground">
              {response.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}
          <HistoryNote response={response} />

          {response.recommendations.length > 0 && (
            <div className="grid gap-4 lg:grid-cols-[1fr_360px]">
              <ol aria-label="Recommendations" className="space-y-3">
                {response.recommendations.map((item) => (
                  <Recommendation key={item.path} item={item} />
                ))}
              </ol>
              {scoped && scoped.nodes.length > 0 && (
                <Card className="self-start">
                  <CardHeader>
                    <CardTitle>In the import graph</CardTitle>
                    <span className="text-xs text-muted-foreground">ringed: selected</span>
                  </CardHeader>
                  <GraphCanvas
                    nodes={scoped.nodes}
                    edges={scoped.edges}
                    emphasized={scoped.emphasized}
                    selected={undefined}
                    onSelect={noop}
                    theme={theme}
                  />
                </Card>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
