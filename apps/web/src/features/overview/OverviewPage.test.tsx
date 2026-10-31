import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { healthyBody, renderWithQuery, stubFetch, stubRoutes } from "@/test/render";

import { useSelectedRepository } from "@/app/useSelectedRepository";
import type { OverviewResponse } from "@/types/generated/OverviewResponse";

import { OverviewPage } from "./OverviewPage";

function Page() {
  return <OverviewPage selection={useSelectedRepository()} />;
}

const repository = { id: 1, path: "/work/app", analyzed_at: 1_704_067_200 };
const oid = "a".repeat(40);
const overview: OverviewResponse = {
  repository,
  run: {
    id: 1,
    target_oid: oid,
    history_limit: 500,
    completed_at: 1_704_067_200,
    parser_version: 0,
    algorithm_version: 1,
    config_hash: "c".repeat(64),
  },
  history: {
    commit_count: 1234,
    merge_commit_count: 7,
    first_commit: { oid, committed_at: 1_704_067_200, subject: "first" },
    last_commit: { oid, committed_at: 1_704_153_600, subject: "last" },
    is_shallow: false,
    is_truncated: false,
  },
  files: {
    tip_file_count: 40,
    tip_typescript_file_count: 30,
    changed_file_count: 25,
    rename_count: 2,
    unreadable_path_count: 1,
  },
  exclusions: [{ reason: "binary", count: 3 }],
  top_churn: [
    {
      path: "src/new.ts",
      exists_at_tip: true,
      commit_count: 9,
      added_lines: 100,
      deleted_lines: 20,
      previous_paths: ["src/old.ts"],
    },
    { path: "src/gone.ts", exists_at_tip: false, commit_count: 2, added_lines: 5, deleted_lines: 5, previous_paths: [] },
  ],
};

function stubAnalyzed(result: OverviewResponse = overview) {
  return stubRoutes({
    "/health": { body: healthyBody },
    "/repositories": { body: [repository] },
    "/repositories/1/overview": { body: result },
    "/repositories/1/commits": {
      body: {
        commits: [
          { position: 0, oid, committed_at: 1_704_067_200, subject: "<b>bold</b> subject", parent_count: 2 },
        ],
        next_cursor: null,
      },
    },
  });
}

describe("OverviewPage", () => {
  it("shows a loading state while the request is in flight", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
    renderWithQuery(<Page />);
    expect(screen.getByLabelText("Loading engine status")).toBeInTheDocument();
  });

  it("renders values from the API response", async () => {
    stubFetch(200, { ...healthyBody, version: "9.8.7" });
    renderWithQuery(<Page />);
    expect(await screen.findByText("9.8.7")).toBeInTheDocument();
    expect(screen.getByText("1 of 1")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByLabelText(/Absolute path/)).toHaveValue("");
  });

  it("explains a degraded server", async () => {
    stubFetch(503, {
      ...healthyBody,
      status: "degraded",
      database: { reachable: false, schema_version: null, expected_schema_version: 1 },
    });
    renderWithQuery(<Page />);
    expect(await screen.findByRole("alert")).toHaveTextContent("not usable");
    expect(screen.getByText("unavailable")).toBeInTheDocument();
  });

  it("explains how to start an unreachable server", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new TypeError("offline"))));
    renderWithQuery(<Page />);
    expect(await screen.findByRole("alert")).toHaveTextContent("cargo run -p atlas-cli -- serve");
  });

  it("renders server-supplied text as text, not HTML", async () => {
    stubFetch(403, {
      error: { code: "forbidden_host", message: "<img src=x onerror=alert(1)>", details: null },
    });
    renderWithQuery(<Page />);
    expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
  });

  it("renders an analyzed repository from the API", async () => {
    stubAnalyzed();
    renderWithQuery(<Page />);
    expect(await screen.findByText("1,234")).toBeInTheDocument();
    expect(screen.getByText("2024-01-01 to 2024-01-02")).toBeInTheDocument();
    expect(screen.getByText("30 TS/TSX")).toBeInTheDocument();
    expect(screen.getByText("2 renames")).toBeInTheDocument();
    expect(screen.getByText("src/new.ts")).toBeInTheDocument();
    expect(screen.getByText("was src/old.ts")).toBeInTheDocument();
    expect(screen.getByText("deleted")).toBeInTheDocument();
    expect(screen.getByText("unreadable paths")).toBeInTheDocument();
  });

  it("states shallow and truncated history", async () => {
    stubAnalyzed({ ...overview, history: { ...overview.history, is_shallow: true, is_truncated: true } });
    renderWithQuery(<Page />);
    expect(await screen.findByText(/Shallow clone/)).toBeInTheDocument();
    expect(screen.getByText(/history limit of 500/)).toBeInTheDocument();
  });

  it("renders commit subjects as text, not HTML", async () => {
    stubAnalyzed();
    renderWithQuery(<Page />);
    expect(await screen.findByText("<b>bold</b> subject")).toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
  });

  it("says when a registered repository is not analyzed", async () => {
    stubRoutes({ "/health": { body: healthyBody }, "/repositories": { body: [repository] } });
    renderWithQuery(<Page />);
    expect(await screen.findByText(/has not been analyzed yet/)).toBeInTheDocument();
  });

  it("waits for the job, then shows the repository", async () => {
    let jobPolls = 0;
    const overviewBeforeDone: string[] = [];
    let analyzing = false;
    const job = (status: string) => ({ id: 1, repository_id: 1, status, error: null });
    const routes = stubRoutes({ "/health": { body: healthyBody } });
    const base = routes.getMockImplementation() as (url: string) => Promise<Response>;
    routes.mockImplementation((url: string, init?: RequestInit) => {
      const respond = (body: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(body), { status }));
      if (url.endsWith("/repositories/open")) {
        expect(init).toMatchObject({ method: "POST", body: JSON.stringify({ path: "/work/app" }) });
        return respond(repository);
      }
      if (url.endsWith("/analyze")) {
        analyzing = true;
        return respond(job("queued"), 202);
      }
      if (url.endsWith("/jobs/1")) {
        jobPolls += 1;
        if (jobPolls < 2) return respond(job("running"));
        analyzing = false;
        return respond(job("completed"));
      }
      if (url.endsWith("/repositories")) return respond(analyzing || jobPolls ? [repository] : []);
      if (url.endsWith("/overview")) {
        if (analyzing) overviewBeforeDone.push(url);
        return analyzing
          ? respond({ error: { code: "not_found", message: "not analyzed", details: null } }, 404)
          : respond(overview);
      }
      if (url.includes("/commits")) return respond({ commits: [], next_cursor: null });
      return base(url);
    });
    renderWithQuery(<Page />);

    await userEvent.type(screen.getByLabelText(/Absolute path/), "/work/app");
    await userEvent.click(screen.getByRole("button", { name: "Analyze" }));

    expect(await screen.findByRole("button", { name: "Analyzing…" })).toBeDisabled();
    expect(await screen.findByText("1,234", {}, { timeout: 3000 })).toBeInTheDocument();
    expect(jobPolls).toBe(2);
    expect(overviewBeforeDone).toEqual([]);
  });

  it("keeps a failed job's reason on screen when the repository list changes", async () => {
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories": { body: [repository] },
      "/repositories/open": { body: repository },
      "/repositories/1/analyze": {
        status: 202,
        body: { id: 1, repository_id: 1, status: "failed", error: "no-such-branch does not name a commit" },
      },
    });
    renderWithQuery(<Page />);
    await screen.findByText(/has not been analyzed yet/);
    await userEvent.click(screen.getByRole("button", { name: "Analyze" }));
    expect(await screen.findByText("no-such-branch does not name a commit")).toBeInTheDocument();
  });

  it("shows why an analysis failed", async () => {
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories/open": { status: 400, body: { error: { code: "invalid_input", message: "/x is not a Git repository", details: null } } },
    });
    renderWithQuery(<Page />);
    await userEvent.type(screen.getByLabelText(/Absolute path/), "/x");
    await userEvent.click(screen.getByRole("button", { name: "Analyze" }));
    expect(await screen.findByText("/x is not a Git repository")).toBeInTheDocument();
  });
});
