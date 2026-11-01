import { useCallback, useMemo, useState } from "react";

import { ApiRequestError } from "@/api/client";
import { useCommits, useDirectoryMap, useGraph } from "@/api/queries";
import type { Theme } from "@/app/theme";
import { useDebounced } from "@/app/useDebounced";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { DirectoryGroup } from "@/types/generated/DirectoryGroup";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

import { CoveragePanel, CycleList } from "./CoveragePanel";
import { DirectoryCanvas, groupLabel } from "./DirectoryCanvas";
import { FileInspector } from "./FileInspector";
import { GraphCanvas } from "./GraphCanvas";

const COMMIT_CHOICES = 200;
/** Above this a file graph is a tangle, so the map comes first. */
const MAP_ABOVE_FILES = 200;
const FOUND_FILES = 100;
const SEGMENT = "h-7 px-3 text-xs aria-pressed:bg-card aria-pressed:text-foreground aria-pressed:shadow-card";
const FIELD = "h-9 rounded-md border bg-card px-3 text-sm";

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
        <span aria-hidden className="h-1 w-6 rounded bg-marker-amber" />
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

  const [view, setView] = useState<"auto" | "files" | "map">("auto");

  const typedDirectory = useDebounced(directory.trim());
  const graph = useGraph(repository?.id, {
    ref: commit,
    directory: typedDirectory || undefined,
    include: showCochange ? "cochange" : undefined,
  });
  const term = useDebounced(search.trim());
  const found = useGraph(term ? repository?.id : undefined, {
    ref: commit,
    directory: typedDirectory || undefined,
    search: term,
    limit: FOUND_FILES,
  });
  const matching = graph.data?.matching_node_count ?? 0;
  const showMap = view === "map" || (view === "auto" && matching > MAP_ABOVE_FILES);
  const map = useDirectoryMap(repository?.id, commit, typedDirectory, showMap);
  const commits = useCommits(repository?.id, COMMIT_CHOICES, true);
  const select = useCallback((path: string | undefined) => setSelected(path), []);

  const data = graph.data;
  const nodes = data?.nodes;
  const edges = useMemo(
    () => data?.edges.filter((edge) => showTypes || edge.edge_type !== "type_only_import"),
    [data, showTypes],
  );
  const listed = term ? (found.data?.nodes ?? []) : (nodes ?? []);
  const open = useCallback((group: DirectoryGroup) => {
    setDirectory(group.path);
    setSelected(undefined);
    setView(group.kind === "loose_files" ? "files" : "auto");
  }, []);

  if (!repository) {
    return (
      <div className="mx-auto max-w-6xl">
        <h1 className="text-2xl font-semibold tracking-tight">Architecture Explorer</h1>
        <p className="mt-2 text-sm text-muted-foreground">Open a repository from the Overview first.</p>
      </div>
    );
  }

  const notAnalyzed = graph.error instanceof ApiRequestError && graph.error.code === "not_found";

  return (
    <div className="mx-auto max-w-6xl space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Architecture Explorer</h1>

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
            onChange={(event) => {
              setDirectory(event.target.value);
              setView("auto");
            }}
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
                  {showMap
                    ? `${(map.data?.groups.length ?? 0).toLocaleString("en-US")} groups of ${data.matching_node_count.toLocaleString("en-US")} files`
                    : `${nodes.length.toLocaleString("en-US")} of ${data.matching_node_count.toLocaleString("en-US")} files`}
                </CardTitle>
                <span className="font-mono text-xs text-muted-foreground">
                  {data.commit.slice(0, 10)} · {data.total_edge_count.toLocaleString("en-US")} imports in total
                </span>
              </CardHeader>
              <div className="flex flex-wrap items-center gap-3 px-4 pt-3 pb-2 text-xs">
                <span className="inline-flex rounded-md bg-muted p-0.5">
                  <Button type="button" variant="ghost" className={SEGMENT} aria-pressed={showMap} onClick={() => setView("map")}>
                    Directory map
                  </Button>
                  <Button type="button" variant="ghost" className={SEGMENT} aria-pressed={!showMap} onClick={() => setView("files")}>
                    Files
                  </Button>
                </span>
                {showMap && map.data && (
                  <nav aria-label="Directory" className="flex flex-wrap items-center gap-1 font-mono text-muted-foreground">
                    <button type="button" className="hover:text-foreground" onClick={() => setDirectory("")}>
                      root
                    </button>
                    {map.data.directory
                      .split("/")
                      .filter(Boolean)
                      .map((part, index, parts) => (
                        <button
                          key={parts.slice(0, index + 1).join("/")}
                          type="button"
                          className="hover:text-foreground"
                          onClick={() => setDirectory(parts.slice(0, index + 1).join("/"))}
                        >
                          / {part}
                        </button>
                      ))}
                  </nav>
                )}
              </div>
              {map.error && showMap && (
                <CardContent>
                  <p role="alert" className="text-sm text-destructive">
                    {map.error.message}
                  </p>
                </CardContent>
              )}
              {nodes.length === 0 ? (
                <CardContent>
                  <p className="text-sm text-muted-foreground">No TS or TSX files match at this commit.</p>
                </CardContent>
              ) : showMap ? (
                map.data ? (
                  <DirectoryCanvas map={map.data} onOpen={open} theme={theme} />
                ) : (
                  <Skeleton aria-label="Loading directory map" className="h-[520px] w-full" />
                )
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
              {showMap ? (
                <p className="border-t px-4 py-2 text-xs text-muted-foreground">
                  Boxes are directories, sized by files. An arrow means files in one import files in the other, and is
                  thicker with more imports. A red border means some files are in an import cycle. Select a box to open
                  it.
                  {map.data && map.data.hidden_group_count > 0
                    ? ` ${map.data.hidden_group_count} smaller groups with ${map.data.hidden_file_count.toLocaleString("en-US")} files are not shown.`
                    : ""}
                </p>
              ) : (
                <Legend />
              )}
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
                    {showMap && !term && map.data && (
                      <ul aria-label="Groups" className="mt-3 space-y-0.5">
                        {map.data.groups.map((group) => (
                          <li key={`${group.kind}:${group.path}`}>
                            <button
                              type="button"
                              onClick={() => open(group)}
                              className="flex w-full justify-between gap-2 rounded px-1 py-0.5 text-left font-mono text-xs hover:bg-muted"
                            >
                              <span className="truncate">{groupLabel(group, map.data.directory)}</span>
                              <span className="text-muted-foreground">{group.file_count.toLocaleString("en-US")}</span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                    <ul aria-label="Files" hidden={showMap && !term} className="mt-3 space-y-0.5">
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
