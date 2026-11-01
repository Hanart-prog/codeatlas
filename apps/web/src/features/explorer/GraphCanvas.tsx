import cytoscape from "cytoscape";
import { useEffect, useRef } from "react";

import type { Theme } from "@/app/theme";
import type { CoChangeEdge } from "@/types/generated/CoChangeEdge";
import type { GraphEdge } from "@/types/generated/GraphEdge";
import type { GraphNode } from "@/types/generated/GraphNode";

import { type DiffState, edgeId } from "./graphDiff";

/** Canvas colors, as Cytoscape cannot read the CSS theme variables. */
const COLORS = {
  dark: { surface: "#221f1c", node: "#8a8378", cycle: "#f0807b", label: "#f1eee9", edge: "#8a8378", selected: "#6ba3e8", cochange: "#e5a057", added: "#6fcf9f", removed: "#f0807b" },
  light: { surface: "#ffffff", node: "#bf89cd", cycle: "#bd3a35", label: "#221f1c", edge: "#797267", selected: "#3f78c8", cochange: "#b07225", added: "#2a7653", removed: "#bd3a35" },
} as const;

const LINE_STYLES: Record<GraphEdge["edge_type"], "solid" | "dashed" | "dotted"> = {
  static_import: "solid",
  re_export: "solid",
  type_only_import: "dashed",
  dynamic_import: "dotted",
};

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

interface Props {
  nodes: Pick<GraphNode, "path" | "in_degree" | "out_degree" | "in_cycle">[];
  edges: Pick<GraphEdge, "source" | "target" | "edge_type">[];
  /** Correlation from history, drawn apart from imports. */
  cochange?: CoChangeEdge[];
  /** Files drawn with a selection ring without being the selected file. */
  emphasized?: string[];
  /** Differences to draw, by file path and by `edgeId`. */
  fileDiff?: ReadonlyMap<string, DiffState>;
  importDiff?: ReadonlyMap<string, DiffState>;
  selected: string | undefined;
  onSelect: (path: string | undefined) => void;
  theme: Theme;
}

const NONE: never[] = [];
const NO_DIFF: ReadonlyMap<string, DiffState> = new Map();

export function GraphCanvas({
  nodes,
  edges,
  cochange = NONE,
  emphasized = NONE,
  fileDiff = NO_DIFF,
  importDiff = NO_DIFF,
  selected,
  onSelect,
  theme,
}: Props) {
  const container = useRef<HTMLDivElement>(null);
  const instance = useRef<cytoscape.Core | null>(null);

  useEffect(() => {
    if (!container.current) return;
    const colors = COLORS[theme];
    const known = new Set(nodes.map((node) => node.path));
    const graph = cytoscape({
      container: container.current,
      elements: [
        ...nodes.map((node) => ({
          data: { id: node.path, label: fileName(node.path), degree: node.in_degree + node.out_degree },
          classes: [
            node.in_cycle ? "cycle" : "",
            emphasized.includes(node.path) ? "emphasized" : "",
            fileDiff.get(node.path) ?? "",
          ].join(" "),
        })),
        ...edges.filter((edge) => known.has(edge.source) && known.has(edge.target)).map((edge) => ({
          data: {
            id: edgeId(edge),
            source: edge.source,
            target: edge.target,
            line: LINE_STYLES[edge.edge_type],
            arrow: edge.edge_type === "re_export" ? "triangle-tee" : "triangle",
          },
          classes: importDiff.get(edgeId(edge)) ?? "",
        })),
        ...cochange.map((pair) => ({
          data: {
            id: `cochange:${pair.file_a}|${pair.file_b}`,
            source: pair.file_a,
            target: pair.file_b,
            shared: pair.shared_commit_count,
          },
          classes: "cochange",
        })),
      ],
      style: [
        {
          selector: "node",
          style: {
            "background-color": colors.surface,
            "border-width": 2,
            "border-color": colors.node,
            label: "data(label)",
            color: colors.label,
            "font-family": '"Inter Variable", system-ui, sans-serif',
            "font-size": 11,
            "text-valign": "bottom",
            "text-margin-y": 4,
            "text-background-color": colors.surface,
            "text-background-opacity": 0.8,
            "text-background-padding": "1px",
            width: "mapData(degree, 0, 12, 16, 38)",
            height: "mapData(degree, 0, 12, 16, 38)",
          },
        },
        { selector: "node.added", style: { "border-color": colors.added, "border-width": 3, shape: "round-rectangle" } },
        { selector: "node.cycle", style: { "border-color": colors.cycle, "border-width": 3, shape: "diamond" } },
        { selector: "node.emphasized", style: { "border-width": 3, "border-color": colors.selected } },
        { selector: "node:selected", style: { "border-width": 3, "border-color": colors.selected } },
        {
          selector: "edge",
          style: {
            width: 1,
            "line-color": colors.edge,
            "line-opacity": 0.6,
            "line-style": "data(line)" as "solid",
            "target-arrow-color": colors.edge,
            "target-arrow-shape": "data(arrow)" as "triangle",
            "curve-style": "bezier",
          },
        },
        {
          selector: "node.removed",
          style: { "background-opacity": 0.15, "border-width": 2, "border-style": "dashed", "border-color": colors.removed },
        },
        { selector: "node.renamed", style: { "border-width": 3, "border-style": "double", "border-color": colors.edge } },
        {
          selector: "edge.added",
          style: { width: 2.5, "line-opacity": 1, "line-color": colors.added, "target-arrow-color": colors.added },
        },
        {
          selector: "edge.removed",
          style: { "line-style": "dashed", "line-opacity": 1, "line-color": colors.removed, "target-arrow-color": colors.removed },
        },
        {
          selector: "edge.cochange",
          style: {
            width: "mapData(shared, 3, 20, 2, 6)",
            "line-color": colors.cochange,
            "line-style": "solid",
            "line-opacity": 1,
            "target-arrow-shape": "none",
            label: "data(shared)",
            color: colors.cochange,
            "font-size": 10,
            "text-background-color": colors.surface,
            "text-background-opacity": 1,
            "text-background-padding": "2px",
          },
        },
      ],
      layout: {
        name: "cose",
        animate: false,
        nodeDimensionsIncludeLabels: true,
        randomize: false,
        padding: 24,
        nodeRepulsion: () => 12_000,
        idealEdgeLength: () => 70,
      },
      wheelSensitivity: 0.3,
    });
    graph.on("tap", (event) => {
      if (event.target === graph) onSelect(undefined);
      else if (event.target.isNode()) onSelect(event.target.id());
    });
    instance.current = graph;
    return () => {
      instance.current = null;
      graph.destroy();
    };
  }, [nodes, edges, cochange, emphasized, fileDiff, importDiff, theme, onSelect]);

  useEffect(() => {
    const graph = instance.current;
    if (!graph) return;
    graph.elements().unselect();
    if (selected) graph.getElementById(selected).select();
  }, [selected, nodes, edges, cochange, emphasized, fileDiff, importDiff, theme]);

  return (
    <div
      ref={container}
      role="img"
      aria-label={`Import graph of ${nodes.length} files and ${edges.length} imports. The text beside it has the same content.`}
      className="h-[520px] w-full"
    />
  );
}
