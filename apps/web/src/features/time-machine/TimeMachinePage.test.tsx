import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { healthyBody, renderWithQuery, stubRoutes } from "@/test/render";
import type { CompareResponse } from "@/types/generated/CompareResponse";

import { comparison, importChange } from "./sample";
import { TimeMachinePage } from "./TimeMachinePage";

vi.mock("@/features/explorer/GraphCanvas", () => ({
  GraphCanvas: ({ nodes, onSelect }: { nodes: { path: string }[]; onSelect: (path: string) => void }) => (
    <button type="button" onClick={() => onSelect("src/a.ts")}>
      canvas of {nodes.length}
    </button>
  ),
}));

const repository = { id: 1, path: "/work/app", analyzed_at: 1 };
const [newest, oldest] = ["1".repeat(40), "2".repeat(40)];

const changed = comparison({
  added_imports: [importChange("src/a.ts", "src/b.ts")],
  removed_imports: [importChange("src/old.ts", "src/b.ts", true)],
  new_cycles: [{ members: ["src/a.ts", "src/b.ts"], type_only: false }],
  changed_cycles: [
    {
      before: { members: ["src/x.ts", "src/y.ts"], type_only: false },
      after: { members: ["src/x.ts", "src/y.ts", "src/z.ts"], type_only: false },
      joined: ["src/z.ts"],
      left: [],
    },
  ],
  renamed_files: [{ from: "src/old.ts", to: "src/new.ts", evidence: "history" }],
  metric_changes: [
    {
      path: "src/a.ts",
      previous_path: null,
      in_degree_before: 1,
      in_degree_after: 2,
      out_degree_before: 0,
      out_degree_after: 1,
      dependents_before: 1,
      dependents_after: 5,
    },
  ],
  notes: ["The root tsconfig.json differs."],
});

const point = (position: number, oid: string, subject: string, file_count: number) => ({
  position,
  commit: { oid, committed_at: position, subject },
  file_count,
  import_count: file_count * 2,
  cycle_count: position,
  cyclic_file_count: 0,
  skipped_file_count: 0,
});

function stub(
  compare: { status?: number; body: unknown } = { body: changed },
  timeline: unknown = { points: [], commit_count: 0 },
) {
  return stubRoutes({
    "/health": { body: healthyBody },
    "/repositories/1/commits": {
      body: {
        commits: [
          { position: 2, oid: newest, committed_at: 2, subject: "Newest", parent_count: 1 },
          { position: 1, oid: oldest, committed_at: 1, subject: "Oldest", parent_count: 1 },
        ],
        next_cursor: null,
      },
    },
    "/repositories/1/timeline": { body: timeline },
    "/repositories/1/compare": compare,
  });
}

async function runCompare() {
  renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
  await screen.findByRole("option", { name: /Oldest/ , selected: true});
  await userEvent.click(screen.getByRole("button", { name: "Compare" }));
}

describe("TimeMachinePage", () => {
  it("asks for a repository first", () => {
    stub();
    renderWithQuery(<TimeMachinePage repository={undefined} theme="dark" />);
    expect(screen.getByText(/Open a repository/)).toBeInTheDocument();
  });

  it("states that imports are not runtime behavior", () => {
    stub();
    renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
    expect(screen.getByText(/not what runs/)).toBeInTheDocument();
  });

  it("defaults to the oldest and newest commits and sends them", async () => {
    const fetchMock = stub();
    await runCompare();
    await screen.findByText("What changed");
    const call = fetchMock.mock.calls.find(([url]) => url.includes("/compare"));
    expect(JSON.parse(call?.[1]?.body as string)).toEqual({ base: oldest, target: newest });
  });

  it("sends the commits the user picks", async () => {
    const fetchMock = stub();
    renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
    await screen.findByRole("option", { name: /Oldest/, selected: true });
    await userEvent.selectOptions(screen.getByLabelText("Before"), newest);
    await userEvent.selectOptions(screen.getByLabelText("After"), oldest);
    await userEvent.click(screen.getByRole("button", { name: "Compare" }));
    await screen.findByText("What changed");
    const call = fetchMock.mock.calls.find(([url]) => url.includes("/compare"));
    expect(JSON.parse(call?.[1]?.body as string)).toEqual({ base: newest, target: oldest });
  });

  it("shows changes, notes, metrics and uncertainty", async () => {
    stub();
    await runCompare();

    expect(await screen.findByText("New cycles (1)")).toBeInTheDocument();
    expect(screen.getByText("Added imports (1)")).toBeInTheDocument();
    expect(screen.getByText("Removed imports (1)")).toBeInTheDocument();
    expect(screen.getByText(/, uncertain/)).toBeInTheDocument();
    expect(screen.getByText("The root tsconfig.json differs.")).toBeInTheDocument();
    expect(screen.getByText("Changed cycles (1)")).toBeInTheDocument();
    expect(screen.getByText(/joined/)).toBeInTheDocument();
    expect(screen.getByText("through history")).toBeInTheDocument();

    const row = screen.getByRole("row", { name: /Source files/ });
    expect(within(row).getByText("+1")).toBeInTheDocument();
  });

  it("opens a file's drawer from the graph and closes it", async () => {
    stub();
    await runCompare();
    await userEvent.click(await screen.findByRole("button", { name: /canvas of/ }));

    expect(await screen.findByText("1 → 2")).toBeInTheDocument();
    expect(screen.getByText("1 → 5")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByText("1 → 5")).not.toBeInTheDocument();
  });

  it("finds a removed import in the drawer through the file's new name", async () => {
    stub();
    await runCompare();
    await userEvent.click((await screen.findAllByRole("button", { name: "src/new.ts" }))[0] as HTMLElement);
    expect(await screen.findByText("Renamed from src/old.ts")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "src/old.ts → src/b.ts" })).toBeInTheDocument();
  });

  it("says so when nothing differs", async () => {
    stub({ body: comparison() });
    await runCompare();
    expect(await screen.findByText(/No differences/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /canvas of/ })).not.toBeInTheDocument();
  });

  it("shows when a list was cut", async () => {
    const cut: CompareResponse = comparison({ added_imports: [importChange("a.ts", "b.ts")], counts: { ...changed.counts, added_imports: 900 } });
    stub({ body: cut });
    await runCompare();
    expect(await screen.findByText("Added imports (900), first 1 shown")).toBeInTheDocument();
  });

  it("reports a failed comparison", async () => {
    stub({ status: 400, body: { error: { code: "invalid_input", message: "unknown commit", details: null } } });
    await runCompare();
    expect(await screen.findByRole("alert")).toHaveTextContent("unknown commit");
  });

  describe("timeline", () => {
    const history = {
      points: [point(0, oldest as string, "Oldest", 3), point(1, newest as string, "Newest", 9)],
      commit_count: 90,
    };

    it("charts each measure on its own axis with a table of the same values", async () => {
      stub({ body: changed }, history);
      renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);

      expect(await screen.findByText("2 of 90 commits")).toBeInTheDocument();
      for (const label of ["Source files", "Imports", "Import cycles"]) {
        expect(screen.getByRole("img", { name: new RegExp(`^${label} by commit`) })).toBeInTheDocument();
      }
      const row = screen.getByRole("row", { name: /Oldest/, hidden: true });
      expect(within(row).getByText("6")).toBeInTheDocument();
    });

    it("sends a commit chosen from the timeline", async () => {
      const fetchMock = stub({ body: changed }, history);
      renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
      await screen.findByText("2 of 90 commits");

      await userEvent.click(screen.getByRole("button", { name: /Oldest/, hidden: true }));
      await userEvent.click(screen.getByRole("button", { name: "Use as After" }));
      await userEvent.click(screen.getByRole("button", { name: "Compare" }));

      await screen.findByText("What changed");
      const call = fetchMock.mock.calls.find(([url]) => url.includes("/compare"));
      expect(JSON.parse(call?.[1]?.body as string)).toEqual({ base: oldest, target: oldest });
    });

    it("can pick a timeline commit older than the commit list", async () => {
      const older = "3".repeat(40);
      stub({ body: changed }, { points: [point(0, older, "Ancient", 1), point(1, newest as string, "Newest", 2)], commit_count: 900 });
      renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
      await screen.findByText("2 of 900 commits");
      expect(within(screen.getByLabelText("Before")).getByRole("option", { name: /Ancient/ })).toBeInTheDocument();
    });

    it("reports a timeline that fails to load", async () => {
      stubRoutes({
        "/health": { body: healthyBody },
        "/repositories/1/commits": { body: { commits: [], next_cursor: null } },
        "/repositories/1/timeline": {
          status: 400,
          body: { error: { code: "invalid_input", message: "timeline failed", details: null } },
        },
      });
      renderWithQuery(<TimeMachinePage repository={repository} theme="dark" />);
      expect(await screen.findByRole("alert")).toHaveTextContent("timeline failed");
    });
  });
});

