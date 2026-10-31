import { type DiffState, edgeId } from "@/features/explorer/graphDiff";
import type { CompareResponse } from "@/types/generated/CompareResponse";
import type { GraphEdge } from "@/types/generated/GraphEdge";

export interface OverlayNode {
  path: string;
  in_degree: number;
  out_degree: number;
  in_cycle: boolean;
}

export interface Overlay {
  nodes: OverlayNode[];
  edges: GraphEdge[];
  fileDiff: Map<string, DiffState>;
  importDiff: Map<string, DiffState>;
}

/** A base path under the name it has at the target, so one file is one node. */
export function carryThroughRenames(response: CompareResponse): (path: string) => string {
  const moved = new Map(response.renamed_files.map((rename) => [rename.from, rename.to]));
  return (path) => moved.get(path) ?? path;
}

/** The changed part of the graph: every differing file and import, on one canvas. */
export function buildOverlay(response: CompareResponse): Overlay {
  const carry = carryThroughRenames(response);
  const fileDiff = new Map<string, DiffState>();
  const importDiff = new Map<string, DiffState>();

  for (const file of response.added_files) fileDiff.set(file.path, "added");
  for (const file of response.removed_files) fileDiff.set(file.path, "removed");
  for (const rename of response.renamed_files) fileDiff.set(rename.to, "renamed");

  const removed = response.removed_imports.map(({ edge }) => ({
    ...edge,
    source: carry(edge.source),
    target: carry(edge.target),
  }));
  const added = response.added_imports.map(({ edge }) => edge);
  for (const edge of removed) importDiff.set(edgeId(edge), "removed");
  for (const edge of added) importDiff.set(edgeId(edge), "added");
  const edges = [...removed, ...added];

  const cyclic = new Set([
    ...response.new_cycles.flatMap((cycle) => cycle.members),
    ...response.resolved_cycles.flatMap((cycle) => cycle.members.map(carry)),
    ...response.changed_cycles.flatMap((change) => [...change.after.members, ...change.before.members.map(carry)]),
  ]);
  const paths = new Set([
    ...fileDiff.keys(),
    ...edges.flatMap((edge) => [edge.source, edge.target]),
    ...cyclic,
  ]);
  const degrees = new Map(
    [...response.removed_files, ...response.added_files].map((file) => [file.path, file] as const),
  );
  const changes = new Map(response.metric_changes.map((change) => [change.path, change]));

  const nodes = [...paths].sort().map((path) => {
    const change = changes.get(path);
    const file = degrees.get(path);
    return {
      path,
      in_degree: change?.in_degree_after ?? file?.in_degree ?? 0,
      out_degree: change?.out_degree_after ?? file?.out_degree ?? 0,
      in_cycle: cyclic.has(path),
    };
  });
  return { nodes, edges, fileDiff, importDiff };
}
