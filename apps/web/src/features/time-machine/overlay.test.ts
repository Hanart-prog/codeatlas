import { describe, expect, it } from "vitest";

import { comparison, graphNode, importChange } from "./sample";
import { buildOverlay } from "./overlay";

describe("buildOverlay", () => {
  it("draws a renamed file once, with its removed import under the new name", () => {
    const overlay = buildOverlay(
      comparison({
        renamed_files: [{ from: "old.ts", to: "new.ts", evidence: "direct" }],
        removed_imports: [importChange("old.ts", "b.ts")],
      }),
    );
    expect(overlay.nodes.map((node) => node.path)).toEqual(["b.ts", "new.ts"]);
    expect(overlay.edges[0]?.source).toBe("new.ts");
    expect(overlay.fileDiff.get("new.ts")).toBe("renamed");
    expect(overlay.importDiff.get("new.ts->b.ts:static_import")).toBe("removed");
  });

  it("marks added and removed files and the endpoints of changed imports", () => {
    const overlay = buildOverlay(
      comparison({
        added_files: [graphNode("a.ts", 1)],
        removed_files: [graphNode("z.ts", 0)],
        added_imports: [importChange("a.ts", "c.ts")],
      }),
    );
    expect(overlay.nodes.map((node) => node.path)).toEqual(["a.ts", "c.ts", "z.ts"]);
    expect(overlay.nodes[0]?.out_degree).toBe(1);
    expect(overlay.fileDiff.get("c.ts")).toBeUndefined();
    expect(overlay.fileDiff.get("z.ts")).toBe("removed");
  });

  it("marks members of new and resolved cycles", () => {
    const overlay = buildOverlay(
      comparison({
        renamed_files: [{ from: "old.ts", to: "new.ts", evidence: "direct" }],
        new_cycles: [{ members: ["a.ts", "b.ts"], type_only: false }],
        resolved_cycles: [{ members: ["old.ts", "c.ts"], type_only: false }],
      }),
    );
    expect(overlay.nodes.filter((node) => node.in_cycle).map((node) => node.path)).toEqual([
      "a.ts",
      "b.ts",
      "c.ts",
      "new.ts",
    ]);
  });

  it("marks members of changed cycles under their new names", () => {
    const overlay = buildOverlay(
      comparison({
        renamed_files: [{ from: "old.ts", to: "new.ts", evidence: "direct" }],
        changed_cycles: [
          {
            before: { members: ["old.ts", "b.ts"], type_only: false },
            after: { members: ["b.ts", "c.ts", "new.ts"], type_only: false },
            joined: ["c.ts"],
            left: [],
          },
        ],
      }),
    );
    expect(overlay.nodes.filter((node) => node.in_cycle).map((node) => node.path)).toEqual(["b.ts", "c.ts", "new.ts"]);
  });
});
