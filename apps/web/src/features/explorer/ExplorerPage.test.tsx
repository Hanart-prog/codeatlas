import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { healthyBody, renderWithQuery, stubRoutes } from "@/test/render";
import type { DirectoryMap } from "@/types/generated/DirectoryMap";
import type { FileDetail } from "@/types/generated/FileDetail";
import type { GraphEdge } from "@/types/generated/GraphEdge";
import type { GraphResponse } from "@/types/generated/GraphResponse";

import { ExplorerPage } from "./ExplorerPage";
import { GraphCanvas } from "./GraphCanvas";

vi.mock("./DirectoryCanvas", async (original) => ({
  ...(await original<typeof import("./DirectoryCanvas")>()),
  DirectoryCanvas: () => <div data-testid="map" />,
}));

vi.mock("./GraphCanvas", () => ({
  GraphCanvas: vi.fn(({ edges }: { edges: GraphEdge[] }) => <div data-testid="canvas">{edges.length} edges</div>),
}));

const repository = { id: 1, path: "/work/app", analyzed_at: 1 };
const oid = "a".repeat(40);

const node = (path: string, in_cycle = false) => ({
  path,
  language: "typescript" as const,
  loc: 10,
  parse_status: "parsed" as const,
  in_degree: 1,
  out_degree: 1,
  in_cycle,
});

const edge = (source: string, target: string, edge_type: GraphEdge["edge_type"]): GraphEdge => ({
  source,
  target,
  edge_type,
  resolution: "relative",
  specifier: "./x",
  line: 1,
});

const graph: GraphResponse = {
  commit: oid,
  nodes: [node("src/a.ts", true), node("src/b.ts", true), node("src/c.ts")],
  edges: [edge("src/a.ts", "src/b.ts", "static_import"), edge("src/b.ts", "src/a.ts", "type_only_import")],
  cycles: [{ members: ["src/a.ts", "src/b.ts"], type_only: true }],
  cochange: [],
  coverage: {
    parsed_file_count: 3,
    partial_file_count: 0,
    skipped_file_count: 1,
    unsupported_file_count: 2,
    excluded_file_count: 4,
    resolved_import_count: 5,
    external_import_count: 6,
    unresolved_import_count: 7,
    unresolved_by_reason: [{ reason: "unknown_alias", count: 7 }],
    nonliteral_dynamic_import_count: 0,
    require_call_count: 8,
    tsconfig: "applied_ignoring_extends",
  },
  matching_node_count: 9,
  total_node_count: 9,
  total_edge_count: 2,
};

const detail: FileDetail = {
  commit: oid,
  node: node("src/a.ts", true),
  imports: [edge("src/a.ts", "src/b.ts", "static_import")],
  imported_by: [edge("src/c.ts", "src/a.ts", "dynamic_import")],
  unresolved: [{ specifier: "<img src=x>", reason: "not_found", line: 2 }],
  external_import_count: 2,
  churn: null,
};

function stubGraph(result: GraphResponse = graph) {
  return stubRoutes({
    "/health": { body: healthyBody },
    "/repositories/1/graph": { body: result },
    "/repositories/1/commits": {
      body: {
        commits: [{ position: 3, oid: "b".repeat(40), committed_at: 1, subject: "Earlier", parent_count: 1 }],
        next_cursor: null,
      },
    },
    "/repositories/1/files": { body: detail },
  });
}

describe("ExplorerPage", () => {
  it("asks for a repository when none is open", () => {
    renderWithQuery(<ExplorerPage repository={undefined} theme="dark" />);
    expect(screen.getByText(/Open a repository from the Overview/)).toBeInTheDocument();
  });

  it("renders the API graph, cycles, coverage and truncation", async () => {
    stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    expect(await screen.findByText("3 of 9 files")).toBeInTheDocument();
    expect(screen.getByTestId("canvas")).toHaveTextContent("2 edges");
    expect(screen.getByText(/types only, gone at run time/)).toBeInTheDocument();
    expect(screen.getByText("undefined alias")).toBeInTheDocument();
    expect(screen.getByText("require calls, not in graph")).toBeInTheDocument();
    expect(screen.getByText("tsconfig.json applied, extends not followed")).toBeInTheDocument();
  });

  it("explains evidence in the legend", async () => {
    stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");
    expect(screen.getByText("A → B: A imports B")).toBeInTheDocument();
    expect(screen.getByText(/Edges show source imports, not what runs/)).toBeInTheDocument();
  });

  it("hides type imports on request without refetching", async () => {
    const fetchMock = stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");
    const graphCalls = () => fetchMock.mock.calls.filter(([url]) => url.includes("/graph")).length;
    const before = graphCalls();

    await userEvent.click(screen.getByRole("checkbox", { name: "Type imports" }));

    expect(screen.getByTestId("canvas")).toHaveTextContent("1 edges");
    expect(graphCalls()).toBe(before);
    expect(vi.mocked(GraphCanvas)).toHaveBeenCalled();
  });

  it("starts a large graph on the directory map and opens a group", async () => {
    const large = { ...graph, matching_node_count: 5000 };
    const map: DirectoryMap = {
      commit: oid,
      directory: "packages",
      groups: [
        { path: "packages/core", kind: "directory", file_count: 4000, cyclic_file_count: 2, internal_import_count: 900 },
        { path: "packages", kind: "loose_files", file_count: 1000, cyclic_file_count: 0, internal_import_count: 10 },
      ],
      links: [{ source: 1, target: 0, import_count: 30 }],
      hidden_group_count: 3,
      hidden_file_count: 12,
      matching_node_count: 5000,
      total_node_count: 5000,
      total_edge_count: 9000,
    };
    const fetchMock = stubRoutes({
      "/health": { body: healthyBody },
      "/repositories/1/graph": { body: large },
      "/repositories/1/graph/directories": { body: map },
      "/repositories/1/commits": { body: { commits: [], next_cursor: null } },
    });
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);

    expect(await screen.findByText("2 groups of 5,000 files")).toBeInTheDocument();
    expect(screen.queryByTestId("canvas")).not.toBeInTheDocument();
    expect(screen.getByText(/3 smaller groups with 12 files/)).toBeInTheDocument();

    const groups = within(screen.getByRole("list", { name: "Groups" }));
    await userEvent.click(groups.getByRole("button", { name: /^core/ }));
    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([url]) => url.includes("graph/directories?directory=packages%2Fcore"))).toBe(true),
    );
  });

  it("lets a large graph be drawn as files on request", async () => {
    const large = { ...graph, matching_node_count: 5000 };
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories/1/graph": { body: large },
      "/repositories/1/commits": { body: { commits: [], next_cursor: null } },
    });
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await userEvent.click(await screen.findByRole("button", { name: "Files" }));
    expect(await screen.findByTestId("canvas")).toBeInTheDocument();
  });

  it("requests the chosen commit and directory", async () => {
    const fetchMock = stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");

    await screen.findByRole("option", { name: /Earlier/ });
    await userEvent.selectOptions(screen.getByLabelText("Commit"), "b".repeat(40));
    await userEvent.type(screen.getByLabelText("Directory"), "src");

    await waitFor(() => {
      const urls = fetchMock.mock.calls.map(([url]) => url);
      expect(urls.some((url) => url.includes(`ref=${"b".repeat(40)}&directory=src`))).toBe(true);
    });
  });

  it("searches on the server, so files beyond the first page can be found", async () => {
    const fetchMock = stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");

    await userEvent.type(screen.getByLabelText("Search files"), "c.ts");
    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([url]) => url.includes("search=c.ts"))).toBe(true),
    );
    const files = within(screen.getByRole("list", { name: "Files" }));
    await userEvent.click(await files.findByRole("button", { name: "src/c.ts" }));

    expect(await screen.findByText("Imports (1)")).toBeInTheDocument();
    expect(screen.getByText("Imported by (1)")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url]) => url.includes("path=src%2Fc.ts"))).toBe(true);
  });

  it("renders unresolved specifiers as text", async () => {
    stubGraph();
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");
    await userEvent.click(screen.getByRole("button", { name: "src/a.ts" }));
    expect(await screen.findByText("<img src=x>")).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
  });

  it("states when no files match", async () => {
    stubGraph({ ...graph, nodes: [], edges: [], matching_node_count: 0 });
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    expect(await screen.findByText(/No TS or TSX files match/)).toBeInTheDocument();
  });

  it("directs unanalyzed repositories to the Overview", async () => {
    stubRoutes({ "/health": { body: healthyBody } });
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    expect(await screen.findByText(/Analyze this repository from the Overview/)).toBeInTheDocument();
  });

  it("requests co-change on demand and shows it apart from imports", async () => {
    const pair = {
      file_a: "src/a.ts",
      file_b: "src/c.ts",
      shared_commit_count: 3,
      file_a_commit_count: 4,
      file_b_commit_count: 5,
    };
    const fetchMock = stubGraph({ ...graph, cochange: [pair] });
    renderWithQuery(<ExplorerPage repository={repository} theme="dark" />);
    await screen.findByText("3 of 9 files");
    expect(fetchMock.mock.calls.some(([url]) => url.includes("include=cochange"))).toBe(false);

    await userEvent.click(screen.getByRole("checkbox", { name: "Co-change" }));
    await userEvent.click(await screen.findByRole("button", { name: "src/a.ts" }));

    expect(fetchMock.mock.calls.some(([url]) => url.includes("include=cochange"))).toBe(true);
    expect(await screen.findByText("Changes with (1)")).toBeInTheDocument();
    expect(screen.getByText("3 of 4 commits")).toBeInTheDocument();
    expect(screen.getByText(/Co-change is correlation, not a dependency/)).toBeInTheDocument();
  });
});
