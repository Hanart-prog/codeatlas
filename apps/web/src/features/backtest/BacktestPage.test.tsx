import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { healthyBody, renderWithQuery, stubRoutes } from "@/test/render";
import type { BacktestResult } from "@/types/generated/BacktestResult";
import type { Estimate } from "@/types/generated/Estimate";
import type { Ranker } from "@/types/generated/Ranker";

import { BacktestPage } from "./BacktestPage";
import { verdict } from "./labels";

const repository = { id: 1, path: "/work/app", analyzed_at: 1 };
const oid = "a".repeat(40);

const estimate = (mean: number, spread = 0.05): Estimate => ({ mean, low: mean - spread, high: mean + spread });

function ranker(name: Ranker, hit: number, spread?: number) {
  return {
    ranker: name,
    metrics: [5, 10].map((k) => ({
      k,
      precision: estimate(hit / 2, spread),
      recall: estimate(hit / 3, spread),
      hit_rate: estimate(k === 5 ? hit : hit + 0.1, spread),
    })),
    mean_candidate_count: 12.5,
    empty_case_count: 2,
  };
}

function result(parts: Partial<BacktestResult> = {}): BacktestResult {
  return {
    target_oid: oid,
    warmup: 100,
    max_cases: 200,
    top_k: [5, 10],
    history_window: 200,
    seed: 1,
    parser_version: 1,
    algorithm_version: 3,
    backtest_version: 1,
    config_hash: "c".repeat(64),
    coverage: 0.825,
    counts: {
      commit_count: 592,
      no_eligible_file_commits: 275,
      bulk_commits: 0,
      warmup_commits: 100,
      candidate_cases: 217,
      too_few_files: 33,
      no_input_at_parent: 1,
      no_target_at_parent: 4,
      evaluable_cases: 179,
      evaluated_cases: 179,
      merge_cases: 0,
      target_files: 654,
      new_files_excluded: 62,
      renamed_targets: 0,
      deleted_targets: 10,
    },
    rankers: [
      ranker("combined", 0.4),
      ranker("co_change", 0.35),
      ranker("reverse_dependency", 0.2),
      ranker("most_changed", 0.75),
      ranker("random", 0.1),
    ],
    seeded_input: ranker("combined", 0.42).metrics,
    trend: [
      {
        from_position: 100,
        to_position: 200,
        case_count: 20,
        rates: [
          { ranker: "combined", k: 5, hit_rate: 0.5 },
          { ranker: "most_changed", k: 5, hit_rate: 0.6 },
        ],
      },
    ],
    cases: [
      {
        position: 100,
        commit: { oid, committed_at: 1, subject: "<b>markup</b> subject" },
        parent: "b".repeat(40),
        input: "src/a.ts",
        targets: ["src/b.ts", "src/c.ts"],
        rankings: [{ ranker: "combined", paths: ["src/b.ts", "src/z.ts"], candidate_count: 2 }],
      },
    ],
    notes: ["Only 21 cases were evaluated."],
    ...parts,
  };
}

function stub(entry: unknown = { created_at: 1, result: result() }) {
  return stubRoutes({
    "/health": { body: healthyBody },
    "/repositories/1/backtests": { body: { entry } },
    "/jobs/1": { body: { id: 1, repository_id: 1, status: "completed", error: null, progress: { done: 3, total: 3 } } },
  });
}

describe("verdict", () => {
  it("says so when a baseline clearly wins", () => {
    expect(verdict(result(), 5)).toMatch(/^Most changed files did better than Change Impact/);
  });

  it("says so when Change Impact clearly wins", () => {
    const rankers = [ranker("combined", 0.8), ranker("most_changed", 0.4)];
    expect(verdict(result({ rankers }), 5)).toMatch(/^Change Impact did better than Most changed files/);
  });

  it("refuses to pick a winner when intervals overlap", () => {
    const rankers = [ranker("combined", 0.7, 0.1), ranker("most_changed", 0.75, 0.1)];
    expect(verdict(result({ rankers }), 5)).toMatch(/cannot separate/);
  });
});

describe("BacktestPage", () => {
  it("asks for a repository first", () => {
    stub();
    renderWithQuery(<BacktestPage repository={undefined} theme="dark" />);
    expect(screen.getByText(/Open a repository/)).toBeInTheDocument();
  });

  it("states what this does not measure, even before any run", async () => {
    stub(null);
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    expect(screen.getByText(/co-edit prediction/)).toBeInTheDocument();
    expect(screen.getByText(/does not measure bugs/)).toBeInTheDocument();
    expect(await screen.findByText(/No backtest has run/)).toBeInTheDocument();
  });

  it("shows the verdict, baselines, counts and configuration", async () => {
    stub();
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);

    expect(await screen.findByText(/did better than Change Impact/)).toBeInTheDocument();
    expect(screen.getByText("Hit rate at 5")).toBeInTheDocument();
    expect(screen.getByText("82.5%")).toBeInTheDocument();
    expect(screen.getByText("179 of 217 candidate cases")).toBeInTheDocument();
    for (const label of ["Change Impact", "Co-change only", "Imports only", "Most changed files", "Random"]) {
      expect(screen.getByRole("rowheader", { name: label })).toBeInTheDocument();
    }
    expect(screen.getByText("New files left out")).toBeInTheDocument();
    expect(screen.getByText("parser 1, analysis 3, backtest 1")).toBeInTheDocument();
    expect(screen.getByText("Only 21 cases were evaluated.")).toBeInTheDocument();
  });

  it("switches every figure to another cutoff", async () => {
    stub();
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    await screen.findByText("Hit rate at 5");
    const row = screen.getByRole("row", { name: /^Change Impact/ });
    expect(within(row).getAllByText("40.0%")[0]).toBeInTheDocument();

    await userEvent.selectOptions(screen.getAllByLabelText("Files inspected")[1] as HTMLElement, "10");
    expect(await screen.findByText("Hit rate at 10")).toBeInTheDocument();
    expect(within(screen.getByRole("row", { name: /^Change Impact/ })).getByText("50.0%")).toBeInTheDocument();
  });

  it("counts hits for a case and shows untrusted subjects as text", async () => {
    stub();
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    expect(await screen.findByText("1 of 2")).toBeInTheDocument();
    expect(screen.getByText(/<b>markup<\/b> subject/)).toBeInTheDocument();
  });

  it("sends the chosen settings and shows the new result", async () => {
    stub(null);
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    await screen.findByText(/No backtest has run/);

    const warmup = screen.getByLabelText("Warm-up commits");
    await userEvent.clear(warmup);
    await userEvent.type(warmup, "20");
    const cutoffs = screen.getByLabelText("Files inspected");
    await userEvent.clear(cutoffs);
    await userEvent.type(cutoffs, "3, 7");

    const posting = stubRoutes({
      "/health": { body: healthyBody },
      "/repositories/1/backtests": { body: { id: 1, repository_id: 1, status: "completed", error: null, progress: null } },
    });
    await userEvent.click(screen.getByRole("button", { name: "Run backtest" }));
    await waitFor(() => expect(posting.mock.calls.some(([, init]) => init?.method === "POST")).toBe(true));
    const call = posting.mock.calls.find(([, init]) => init?.method === "POST");
    expect(JSON.parse(call?.[1]?.body as string)).toEqual({ warmup: 20, max_cases: 200, top_k: [3, 7] });
  });

  it("explains a run with no cases and hides empty charts", async () => {
    const empty = result({
      rankers: [],
      cases: [],
      trend: [],
      notes: ["No commit could be evaluated: 64 eligible commits went to warm-up and 0 remained."],
      counts: { ...result().counts, evaluated_cases: 0 },
    });
    stub({ created_at: 1, result: empty });
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    expect(await screen.findByText(/No commit could be evaluated/)).toBeInTheDocument();
    expect(screen.queryByText("Against the baselines")).not.toBeInTheDocument();
  });

  it("reports a failed job", async () => {
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories/1/backtests": { status: 400, body: { error: { code: "invalid_input", message: "warmup must be at least 1", details: null } } },
    });
    renderWithQuery(<BacktestPage repository={repository} theme="dark" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("warmup must be at least 1");
  });
});
