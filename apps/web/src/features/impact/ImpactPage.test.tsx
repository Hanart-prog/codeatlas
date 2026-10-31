import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { healthyBody, renderWithQuery, stubRoutes } from "@/test/render";
import type { ImpactResponse } from "@/types/generated/ImpactResponse";

import { ImpactPage } from "./ImpactPage";

vi.mock("@/features/explorer/GraphCanvas", () => ({
  GraphCanvas: ({ nodes }: { nodes: unknown[] }) => <div data-testid="canvas">{nodes.length} nodes</div>,
}));

const repository = { id: 1, path: "/work/app", analyzed_at: 1 };
const oid = "a".repeat(40);

const response: ImpactResponse = {
  cutoff: oid,
  selected: [{ path: "src/a.ts", status: "analyzed", commit_count: 4, direct_dependent_count: 1 }],
  recommendations: [
    {
      rank: 1,
      path: "src/b.ts",
      score: 0.9,
      static_score: 1,
      historical_score: 0.75,
      evidence: ["static_dependency", "historical_co_change"],
      static_evidence: { distance: 1, path: ["src/b.ts", "src/a.ts"] },
      historical_evidence: {
        selected: "src/a.ts",
        shared_commit_count: 3,
        selected_commit_count: 4,
        candidate_commit_count: 5,
        conditional_probability: 0.75,
        jaccard: 0.5,
        base_rate: 0.25,
        lift: 3,
        commits: [{ oid, committed_at: 1, subject: "<b>markup</b> subject" }],
      },
      caveats: [],
    },
    {
      rank: 2,
      path: "src/c.ts",
      score: 0.24,
      static_score: 0,
      historical_score: 0.6,
      evidence: ["historical_co_change"],
      static_evidence: null,
      historical_evidence: null,
      caveats: ["No import path connects these files, so this is correlation in history only."],
    },
  ],
  candidate_count: 7,
  history: {
    commit_count: 13,
    eligible_commit_count: 11,
    bulk_commit_count: 1,
    no_eligible_file_commit_count: 1,
    merge_commit_count: 1,
    bulk_threshold: 50,
    min_shared_commits: 3,
    is_shallow: true,
    is_truncated: true,
  },
  max_depth: 3,
  static_weight: 0.6,
  historical_weight: 0.4,
  notes: ["No file imports src/a.ts directly at this commit."],
};

function stub(impact: { status?: number; body: unknown } = { body: response }) {
  return stubRoutes({
    "/health": { body: healthyBody },
    "/repositories/1/graph": { body: { nodes: [], edges: [], cochange: [] } },
    "/repositories/1/commits": {
      body: {
        commits: [{ position: 3, oid: "b".repeat(40), committed_at: 1, subject: "Earlier", parent_count: 1 }],
        next_cursor: null,
      },
    },
    "/repositories/1/impact": impact,
  });
}

async function rank() {
  await userEvent.type(screen.getByLabelText("Add a file"), "src/a.ts");
  await userEvent.click(screen.getByRole("button", { name: "Add" }));
  await userEvent.click(screen.getByRole("button", { name: "Rank files" }));
}

describe("ImpactPage", () => {
  it("asks for a repository when none is open", () => {
    renderWithQuery(<ImpactPage repository={undefined} theme="dark" />);
    expect(screen.getByText(/Open a repository from the Overview/)).toBeInTheDocument();
  });

  it("states what a score is before any result", () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    expect(screen.getByText(/not a probability of failure/)).toBeInTheDocument();
  });

  it("needs a file before ranking", () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    expect(screen.getByRole("button", { name: "Rank files" })).toBeDisabled();
  });

  it("sends the chosen cutoff and options", async () => {
    const fetchMock = stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await screen.findByRole("option", { name: /Earlier/ });
    await userEvent.selectOptions(screen.getByLabelText("As of commit"), "b".repeat(40));
    await rank();
    await screen.findByText("0.900");

    const call = fetchMock.mock.calls.find(([url]) => url.endsWith("/impact"));
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({
      selected_paths: ["src/a.ts"],
      max_depth: 3,
      top_k: 10,
      cutoff: "b".repeat(40),
    });
  });

  it("shows rankings with separate evidence and counts", async () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await rank();

    const items = within(await screen.findByRole("list", { name: "Recommendations" })).getAllByRole("listitem");
    const first = items[0] as HTMLElement;
    expect(within(first).getByText("#1")).toBeInTheDocument();
    expect(within(first).getByText("0.900")).toBeInTheDocument();
    expect(within(first).getByText("Imports, 1 hop")).toBeInTheDocument();
    expect(within(first).getByText("3 of 4 commits")).toBeInTheDocument();
    expect(within(first).getByText(/changes in 25% of all commits · lift 3.0/)).toBeInTheDocument();
    expect(screen.getByText(/correlation in history only/)).toBeInTheDocument();
  });

  it("reports what the history covers and its gaps", async () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await rank();
    expect(await screen.findByText(/13 first-parent commits read, 11 eligible/)).toBeInTheDocument();
    expect(screen.getByText(/Shallow clone/)).toBeInTheDocument();
    expect(screen.getByText(/The window cut off older history/)).toBeInTheDocument();
    expect(screen.getByText("No file imports src/a.ts directly at this commit.")).toBeInTheDocument();
  });

  it("renders commit subjects as text", async () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await rank();
    expect(await screen.findByText("<b>markup</b> subject")).toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
  });

  it("explains an empty result instead of implying safety", async () => {
    const note = "No file has import or co-change evidence here. That is an absence of evidence, not a sign that the change is safe.";
    stub({ body: { ...response, recommendations: [], notes: [note] } });
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await rank();
    expect(await screen.findByText(note)).toBeInTheDocument();
  });

  it("shows why a request failed", async () => {
    stub({ status: 404, body: { error: { code: "not_found", message: "src/a.ts does not exist", details: null } } });
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await rank();
    expect(await screen.findByRole("alert")).toHaveTextContent("does not exist");
  });

  it("removes a selected file", async () => {
    stub();
    renderWithQuery(<ImpactPage repository={repository} theme="dark" />);
    await userEvent.type(screen.getByLabelText("Add a file"), "src/a.ts");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove src/a.ts" }));
    expect(screen.queryByRole("list", { name: "Selected files" })).not.toBeInTheDocument();
  });
});
