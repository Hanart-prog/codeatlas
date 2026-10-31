import { describe, expect, it, vi } from "vitest";

import { healthyBody, stubFetch } from "@/test/render";

import { ApiRequestError, fetchHealth, fetchOverview, openRepository, startAnalysis } from "./client";

describe("fetchHealth", () => {
  it("requests the versioned health route", async () => {
    const fetchMock = stubFetch(200, healthyBody);
    await expect(fetchHealth()).resolves.toEqual(healthyBody);
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/health", {});
  });

  it("treats a 503 health body as a degraded result, not an error", async () => {
    const degraded = { ...healthyBody, status: "degraded" };
    stubFetch(503, degraded);
    await expect(fetchHealth()).resolves.toEqual(degraded);
  });

  it("surfaces the API error envelope", async () => {
    stubFetch(403, { error: { code: "forbidden_host", message: "nope", details: null } });
    await expect(fetchHealth()).rejects.toMatchObject({ code: "forbidden_host", message: "nope" });
  });

  it("reports non-JSON responses, such as the dev proxy's, as unreachable", async () => {
    stubFetch(500, "<html>Bad gateway</html>");
    await expect(fetchHealth()).rejects.toMatchObject({ code: "unreachable" });
  });

  it("reports network failures as unreachable", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new TypeError("Failed to fetch"))));
    const error = await fetchHealth().catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiRequestError);
    expect(error).toMatchObject({ code: "unreachable" });
  });
});

describe("repository requests", () => {
  it("treats a job's empty error field as a normal response", async () => {
    const fetchMock = stubFetch(200, {});
    const job = { id: 1, repository_id: 1, status: "queued", error: null };
    fetchMock.mockImplementation(() => Promise.resolve(new Response(JSON.stringify(job), { status: 202 })));
    await expect(startAnalysis(1)).resolves.toEqual(job);
  });

  it("posts JSON when opening a repository", async () => {
    const fetchMock = stubFetch(200, {});
    fetchMock.mockImplementation(() => Promise.resolve(new Response(JSON.stringify({ id: 1 }))));
    await openRepository("/work/app");
    expect(fetchMock.mock.calls[0]).toEqual([
      "/api/v1/repositories/open",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: "/work/app" }),
      },
    ]);
  });

  it("carries API error codes to callers", async () => {
    const fetchMock = stubFetch(200, {});
    const body = { error: { code: "not_found", message: "no such repository", details: null } };
    fetchMock.mockImplementation(() => Promise.resolve(new Response(JSON.stringify(body), { status: 404 })));
    await expect(fetchOverview(9, new AbortController().signal)).rejects.toMatchObject({ code: "not_found" });
  });
});
