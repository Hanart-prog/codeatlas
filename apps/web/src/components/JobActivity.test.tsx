import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { stubRoutes } from "@/test/render";
import type { JobResponse } from "@/types/generated/JobResponse";

import { JobActivity, progressText } from "./JobActivity";

const job: JobResponse = {
  id: 7,
  repository_id: 1,
  kind: "analysis",
  status: "running",
  error: null,
  progress: { done: 1200, total: 2000, unit: "commits" },
};

describe("JobActivity", () => {
  it("says what the job is doing at each stage", () => {
    expect(progressText(job)).toBe("Read 1,200 of 2,000 commits");
    expect(progressText({ ...job, progress: { done: 3, total: 9, unit: "files" } })).toBe("Parsed 3 of 9 files");
    expect(progressText({ ...job, progress: { done: 3, total: 9, unit: "cases" } })).toBe("Evaluated 3 of 9 cases");
    expect(progressText({ ...job, status: "queued", progress: null })).toBe("Waiting for a free slot");
    expect(progressText({ ...job, progress: null })).toBe("Starting");
  });

  it("asks the server to stop the job", async () => {
    const fetchMock = stubRoutes({ "/jobs/7/cancel": { body: job } });
    render(<JobActivity job={job} />);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByText("Stopping…")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url, init]) => url.endsWith("/jobs/7/cancel") && init?.method === "POST")).toBe(true);
  });

  it("offers Cancel again if the request fails", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("down"))));
    render(<JobActivity job={job} />);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not cancel");
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled());
  });
});
