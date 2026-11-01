import cytoscape from "cytoscape";
import { useEffect, useRef } from "react";

import type { Theme } from "@/app/theme";
import type { DirectoryGroup } from "@/types/generated/DirectoryGroup";
import type { DirectoryMap } from "@/types/generated/DirectoryMap";

const COLORS = {
  dark: { node: "#334155", border: "#94a3b8", cycle: "#f87171", label: "#e2e8f0", edge: "#64748b" },
  light: { node: "#e2e8f0", border: "#64748b", cycle: "#dc2626", label: "#1e293b", edge: "#94a3b8" },
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
            "border-width": 1,
            "border-color": colors.border,
            label: "data(label)",
            color: colors.label,
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
            opacity: 0.7,
          },
        },
      ],
      layout: { name: "cose", animate: false, nodeDimensionsIncludeLabels: true, randomize: false },
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
