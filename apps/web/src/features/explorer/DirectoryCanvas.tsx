import cytoscape from "cytoscape";
import { useEffect, useRef } from "react";

import type { Theme } from "@/app/theme";
import type { DirectoryGroup } from "@/types/generated/DirectoryGroup";
import type { DirectoryMap } from "@/types/generated/DirectoryMap";

const COLORS = {
  dark: { node: "#2e2a26", border: "#8a8378", cycle: "#f0807b", label: "#f1eee9", edge: "#8a8378" },
  light: { node: "#ffffff", border: "#bf89cd", cycle: "#bd3a35", label: "#221f1c", edge: "#797267" },
} as const;

export function groupLabel(group: DirectoryGroup, root: string): string {
  if (group.kind === "loose_files") return "Files here";
  return group.path.slice(root ? root.length + 1 : 0);
}

interface Props {
  map: DirectoryMap;
  onOpen: (group: DirectoryGroup) => void;
  theme: Theme;
}

export function DirectoryCanvas({ map, onOpen, theme }: Props) {
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!container.current) return;
    const colors = COLORS[theme];
    const largest = Math.max(1, ...map.groups.map((group) => group.file_count));
    const busiest = Math.max(1, ...map.links.map((link) => link.import_count));
    const graph = cytoscape({
      container: container.current,
      elements: [
        ...map.groups.map((group, index) => ({
          data: {
            id: String(index),
            label: `${groupLabel(group, map.directory)}\n${group.file_count.toLocaleString("en-US")} files`,
            size: 30 + 50 * Math.sqrt(group.file_count / largest),
          },
          classes: group.cyclic_file_count > 0 ? "cycle" : "",
        })),
        ...map.links.map((link) => ({
          data: {
            id: `${link.source}-${link.target}`,
            source: String(link.source),
            target: String(link.target),
            width: 1 + 5 * Math.sqrt(link.import_count / busiest),
          },
        })),
      ],
      style: [
        {
          selector: "node",
          style: {
            shape: "round-rectangle",
            "background-color": colors.node,
            "corner-radius": "14",
            "border-width": 2,
            "border-color": colors.border,
            label: "data(label)",
            color: colors.label,
            "font-family": '"Inter Variable", system-ui, sans-serif',
            "font-size": 11,
            "text-wrap": "wrap",
            "text-valign": "bottom",
            "text-margin-y": 4,
            width: "data(size)",
            height: "data(size)",
          },
        },
        { selector: "node.cycle", style: { "border-width": 3, "border-color": colors.cycle } },
        {
          selector: "edge",
          style: {
            width: "data(width)",
            "line-color": colors.edge,
            "target-arrow-color": colors.edge,
            "target-arrow-shape": "triangle",
            "curve-style": "bezier",
            "line-opacity": 0.5,
          },
        },
      ],
      layout: {
        name: "cose",
        animate: false,
        nodeDimensionsIncludeLabels: true,
        randomize: false,
        padding: 32,
        nodeRepulsion: () => 40_000,
        idealEdgeLength: () => 110,
      },
      wheelSensitivity: 0.3,
    });
    graph.on("tap", "node", (event) => {
      const group = map.groups[Number(event.target.id())];
      if (group) onOpen(group);
    });
    return () => graph.destroy();
  }, [map, onOpen, theme]);

  return (
    <div
      ref={container}
      role="img"
      aria-label={`Map of ${map.groups.length} groups of files and the imports between them. The list beside it has the same groups.`}
      className="h-[520px] w-full"
    />
  );
}
