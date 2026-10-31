import { useCallback, useMemo, useState } from "react";

import { ApiRequestError } from "@/api/client";
import { useCommits, useGraph } from "@/api/queries";
import type { Theme } from "@/app/theme";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

import { CoveragePanel, CycleList } from "./CoveragePanel";
import { FileInspector } from "./FileInspector";
import { GraphCanvas } from "./GraphCanvas";

const COMMIT_CHOICES = 200;
const FIELD = "h-8 rounded-md border bg-background px-2 text-sm";

function LegendLine({ dash, label }: { dash: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <svg aria-hidden width="24" height="6">
        <line x1="0" y1="3" x2="24" y2="3" stroke="currentColor" strokeWidth="1.5" strokeDasharray={dash} />
      </svg>
      {label}
    </span>
  );
}

function Legend() {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t px-4 py-2 text-xs text-muted-foreground">
      <span>A → B: A imports B</span>
      <LegendLine dash="" label="import or re-export" />
      <LegendLine dash="5 3" label="type import" />
      <LegendLine dash="1.5 3" label="dynamic import" />
      <span className="flex items-center gap-1.5">
        <span aria-hidden className="size-2.5 rotate-45 bg-destructive" />
        in a cycle
      </span>
      <span className="flex items-center gap-1.5">
        <span aria-hidden className="h-1 w-6 rounded bg-amber-500" />
        co-change, with shared commits
      </span>
      <span>Edges show source imports, not what runs. Co-change is correlation, not a dependency.</span>
    </div>
  );
}

interface Props {
  repository: RepositoryEntry | undefined;
  theme: Theme;
}

export function ExplorerPage({ repository, theme }: Props) {
  const [commit, setCommit] = useState<string>();
  const [directory, setDirectory] = useState("");
  const [search, setSearch] = useState("");
  const [showTypes, setShowTypes] = useState(true);
  const [showCochange, setShowCochange] = useState(false);
  const [selected, setSelected] = useState<string>();

  const graph = useGraph(repository?.id, {
    ref: commit,
    directory: directory.trim() || undefined,
    include: showCochange ? "cochange" : undefined,
  });
  const commits = useCommits(repository?.id, COMMIT_CHOICES, true);
  const select = useCallback((path: string | undefined) => setSelected(path), []);

  const data = graph.data;
  const nodes = data?.nodes;
  const edges = useMemo(
    () => data?.edges.filter((edge) => showTypes || edge.edge_type !== "type_only_import"),
    [data, showTypes],
  );
  const listed = useMemo(() => {
    const term = search.trim().toLowerCase();
    return nodes?.filter((node) => node.path.toLowerCase().includes(term)) ?? [];
  }, [nodes, search]);

  if (!repository) {
    return (
      <div className="mx-auto max-w-4xl">
        <h1 className="text-xl font-semibold tracking-tight">Architecture Explorer</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }

  const notAnalyzed = graph.error instanceof ApiRequestError && graph.error.code === "not_found";

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <h1 className="text-xl font-semibold tracking-tight">Architecture Explorer</h1>

      <div className="flex flex-wrap items-end gap-3">
        <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
          Commit
          <select
            className={FIELD}
            value={commit ?? ""}
            onChange={(event) => {
              setCommit(event.target.value || undefined);
              setSelected(undefined);
            }}
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
          Directory
          <input
            className={cn(FIELD, "w-44 font-mono")}
            value={directory}
            onChange={(event) => setDirectory(event.target.value)}
            placeholder="src/auth"
            spellCheck={false}
          />
        </label>
        <label className="flex h-8 items-center gap-2 text-sm">
          <input type="checkbox" checked={showTypes} onChange={(event) => setShowTypes(event.target.checked)} />
          Type imports
        </label>
        <label className="flex h-8 items-center gap-2 text-sm">
          <input type="checkbox" checked={showCochange} onChange={(event) => setShowCochange(event.target.checked)} />
          Co-change
        </label>
      </div>

      {notAnalyzed && <p className="text-sm text-muted-foreground">Analyze this repository from the Overview first.</p>}
      {graph.error && !notAnalyzed && (
        <p role="alert" className="text-sm text-destructive">
          {graph.error.message}
        </p>
      )}
      {graph.isLoading && <Skeleton aria-label="Loading graph" className="h-[520px] w-full" />}

      {data && nodes && edges && (
        <>
          <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
            <Card>
              <CardHeader>
                <CardTitle>
                  {nodes.length.toLocaleString("en-US")} of {data.matching_node_count.toLocaleString("en-US")} files
                </CardTitle>
                <span className="font-mono text-xs text-muted-foreground">
                  {data.commit.slice(0, 10)} · {data.total_edge_count.toLocaleString("en-US")} imports in total
                </span>
              </CardHeader>
              {nodes.length === 0 ? (
                <CardContent>
                  <p className="text-sm text-muted-foreground">No TS or TSX files match at this commit.</p>
                </CardContent>
              ) : (
                <GraphCanvas
                  nodes={nodes}
                  edges={edges}
                  cochange={data.cochange}
                  selected={selected}
                  onSelect={select}
                  theme={theme}
                />
              )}
              <Legend />
            </Card>

            <Card>
              <CardContent className="max-h-[610px] overflow-y-auto">
                {selected ? (
                  <FileInspector
                    repositoryId={repository.id}
                    path={selected}
                    commit={commit}
                    onSelect={select}
                    cochange={data.cochange}
                    showCochange={showCochange}
                  />
                ) : (
                  <>
                    <input
                      aria-label="Search files"
                      className={cn(FIELD, "w-full font-mono")}
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      placeholder="Search files"
                      spellCheck={false}
                    />
                    <ul aria-label="Files" className="mt-3 space-y-0.5">
                      {listed.map((node) => (
                        <li key={node.path}>
                          <button
                            type="button"
                            title={node.path}
                            onClick={() => setSelected(node.path)}
                            className="block w-full truncate rounded px-1 py-0.5 text-left font-mono text-xs hover:bg-muted"
                          >
                            {node.path}
                          </button>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
              </CardContent>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Import cycles</CardTitle>
              </CardHeader>
              <CardContent>
                <CycleList cycles={data.cycles} />
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle>Coverage</CardTitle>
              </CardHeader>
              <CardContent>
                <CoveragePanel coverage={data.coverage} />
              </CardContent>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
