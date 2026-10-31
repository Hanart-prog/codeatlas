import type { GraphEdge } from "@/types/generated/GraphEdge";

/** How a file or import differs between two commits. */
export type DiffState = "added" | "removed" | "renamed";

export function edgeId(edge: Pick<GraphEdge, "source" | "target" | "edge_type">): string {
  return `${edge.source}->${edge.target}:${edge.edge_type}`;
}
