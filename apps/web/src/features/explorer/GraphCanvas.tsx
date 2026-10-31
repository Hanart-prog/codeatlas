import cytoscape from "cytoscape";
import { useEffect, useRef } from "react";

import type { Theme } from "@/app/theme";
import type { CoChangeEdge } from "@/types/generated/CoChangeEdge";
import type { GraphEdge } from "@/types/generated/GraphEdge";
import type { GraphNode } from "@/types/generated/GraphNode";

/** Canvas colors, as Cytoscape cannot read the CSS theme variables. */
const COLORS = {
  dark: { node: "#94a3b8", cycle: "#f87171", label: "#e2e8f0", edge: "#64748b", selected: "#5eead4", cochange: "#fbbf24" },
  light: { node: "#64748b", cycle: "#dc2626", label: "#1e293b", edge: "#94a3b8", selected: "#0f766e", cochange: "#b45309" },
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
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** Correlation from history, drawn apart from imports. */
  cochange?: CoChangeEdge[];
  /** Files drawn with a selection ring without being the selected file. */
  emphasized?: string[];
  selected: string | undefined;
  onSelect: (path: string | undefined) => void;
  theme: Theme;
}

const NONE: never[] = [];

export function GraphCanvas({
  nodes,
  edges,
  cochange = NONE,
  emphasized = NONE,
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
          classes: [node.in_cycle ? "cycle" : "", emphasized.includes(node.path) ? "emphasized" : ""].join(" "),
        })),
        ...edges.filter((edge) => known.has(edge.source) && known.has(edge.target)).map((edge) => ({
          data: {
            id: `${edge.source}->${edge.target}:${edge.edge_type}`,
            source: edge.source,
            target: edge.target,
            line: LINE_STYLES[edge.edge_type],
            arrow: edge.edge_type === "re_export" ? "triangle-tee" : "triangle",
          },
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
            "background-color": colors.node,
            label: "data(label)",
            color: colors.label,
            "font-size": 11,
            "text-valign": "bottom",
            "text-margin-y": 4,
            width: "mapData(degree, 0, 12, 14, 34)",
            height: "mapData(degree, 0, 12, 14, 34)",
          },
        },
        { selector: "node.cycle", style: { "background-color": colors.cycle, shape: "diamond" } },
        { selector: "node.emphasized", style: { "border-width": 3, "border-color": colors.selected } },
        { selector: "node:selected", style: { "border-width": 3, "border-color": colors.selected } },
        {
          selector: "edge",
          style: {
            width: 1.5,
            "line-color": colors.edge,
            "line-style": "data(line)" as "solid",
            "target-arrow-color": colors.edge,
            "target-arrow-shape": "data(arrow)" as "triangle",
            "curve-style": "bezier",
          },
        },
        {
          selector: "edge.cochange",
          style: {
            width: "mapData(shared, 3, 20, 2, 6)",
            "line-color": colors.cochange,
            "line-style": "solid",
            "target-arrow-shape": "none",
            label: "data(shared)",
            color: colors.cochange,
            "font-size": 10,
            "text-background-color": theme === "dark" ? "#14181f" : "#ffffff",
            "text-background-opacity": 1,
            "text-background-padding": "2px",
          },
        },
      ],
      layout: { name: "cose", animate: false, nodeDimensionsIncludeLabels: true, randomize: false },
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
  }, [nodes, edges, cochange, emphasized, theme, onSelect]);

  useEffect(() => {
    const graph = instance.current;
    if (!graph) return;
    graph.elements().unselect();
    if (selected) graph.getElementById(selected).select();
  }, [selected, nodes, edges, cochange, emphasized, theme]);

  return (
    <div
      ref={container}
      role="img"
      aria-label={`Import graph of ${nodes.length} files and ${edges.length} imports. The text beside it has the same content.`}
      className="h-[520px] w-full"
    />
  );
}
