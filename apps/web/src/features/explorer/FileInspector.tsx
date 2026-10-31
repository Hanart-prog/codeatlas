import { useFileDetail } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import type { GraphEdge } from "@/types/generated/GraphEdge";

import { EDGE_LABELS, PARSE_LABELS, REASON_LABELS } from "./labels";

function EdgeList({
  title,
  edges,
  side,
  onSelect,
}: {
  title: string;
  edges: GraphEdge[];
  side: "source" | "target";
  onSelect: (path: string) => void;
}) {
  return (
    <section>
      <h3 className="text-xs text-muted-foreground">
        {title} ({edges.length})
      </h3>
      <ul className="mt-1 space-y-0.5">
        {edges.map((edge) => (
          <li key={`${edge[side]}:${edge.edge_type}`} className="flex items-baseline gap-2">
            <button
              type="button"
              onClick={() => onSelect(edge[side])}
              title={edge[side]}
              className="min-w-0 flex-1 truncate text-left font-mono text-xs hover:underline"
            >
              {edge[side]}
            </button>
            <span className="shrink-0 text-xs text-muted-foreground">{EDGE_LABELS[edge.edge_type]}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

interface Props {
  repositoryId: number;
  path: string;
  commit: string | undefined;
  onSelect: (path: string | undefined) => void;
}

export function FileInspector({ repositoryId, path, commit, onSelect }: Props) {
  const { data, error, isPending } = useFileDetail(repositoryId, path, commit);

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-2">
        <h2 className="min-w-0 break-all font-mono text-sm">{path}</h2>
        <Button variant="ghost" className="h-6 px-2 text-xs" onClick={() => onSelect(undefined)}>
          Close
        </Button>
      </div>
      {isPending && <Skeleton aria-label="Loading file" className="h-24 w-full" />}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error.message}
        </p>
      )}
      {data && (
        <>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
            <dt className="text-muted-foreground">Lines</dt>
            <dd className="text-right font-mono">{data.node.loc}</dd>
            <dt className="text-muted-foreground">Commits changing it</dt>
            <dd className="text-right font-mono">{data.churn?.commit_count ?? "none indexed"}</dd>
            <dt className="text-muted-foreground">Package imports</dt>
            <dd className="text-right font-mono">{data.external_import_count}</dd>
          </dl>
          <p className="text-xs text-muted-foreground">
            {PARSE_LABELS[data.node.parse_status]}
            {data.node.in_cycle && " · in an import cycle"}
          </p>
          <EdgeList title="Imports" edges={data.imports} side="target" onSelect={onSelect} />
          <EdgeList title="Imported by" edges={data.imported_by} side="source" onSelect={onSelect} />
          {data.unresolved.length > 0 && (
            <section>
              <h3 className="text-xs text-muted-foreground">Unresolved ({data.unresolved.length})</h3>
              <ul className="mt-1 space-y-0.5">
                {data.unresolved.map((item) => (
                  <li key={`${item.line}:${item.specifier}`} className="flex items-baseline gap-2 text-xs">
                    <span className="min-w-0 flex-1 truncate font-mono">{item.specifier}</span>
                    <span className="shrink-0 text-muted-foreground">{REASON_LABELS[item.reason]}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </div>
  );
}
