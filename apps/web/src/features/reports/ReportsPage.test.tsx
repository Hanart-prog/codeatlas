import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderWithQuery, stubRoutes } from "@/test/render";

import { ReportsPage } from "./ReportsPage";

const repository = { id: 1, path: "/work/app", analyzed_at: 1 };
const report = {
  provenance: {
    tool_version: "0.1.0",
    generated_at: 1,
    repository_name: "app",
    commit: "a".repeat(40),
    history_limit: 500,
    parser_version: 1,
    algorithm_version: 4,
    backtest_version: 1,
    config_hash: "c".repeat(64),
  },
  history: { commit_count: 1200 },
  top_churn: [],
  architecture: null,
  timeline: [],
  backtest: null,
};

describe("ReportsPage", () => {
  it("links to both formats and says what was left out and why", async () => {
    stubRoutes({ "/repositories/1/report": { body: report } });
    renderWithQuery(<ReportsPage repository={repository} />);

    expect(await screen.findByRole("link", { name: "Download HTML" })).toHaveAttribute(
      "href",
      "/api/v1/repositories/1/report?format=html",
    );
    expect(screen.getByRole("link", { name: "Download JSON" })).toHaveAttribute(
      "href",
      "/api/v1/repositories/1/report?format=json",
    );
    expect(screen.getByText(/1,200 first-parent commits/)).toBeInTheDocument();
    expect(screen.getByText(/over a graph limit/)).toBeInTheDocument();
    expect(screen.getByText(/none has run on this commit/)).toBeInTheDocument();
  });

  it("explains an unanalyzed repository rather than failing", async () => {
    stubRoutes({
      "/repositories/1/report": {
        status: 404,
        body: { error: { code: "not_found", message: "not analyzed", details: null } },
      },
    });
    renderWithQuery(<ReportsPage repository={repository} />);
    expect(await screen.findByText("Analyze this repository from the Overview first.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Download HTML" })).not.toBeInTheDocument();
  });

  it("shows why a report could not be built", async () => {
    stubRoutes({
      "/repositories/1/report": {
        status: 504,
        body: { error: { code: "timed_out", message: "this took too long", details: null } },
      },
    });
    renderWithQuery(<ReportsPage repository={repository} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("this took too long");
  });
});
