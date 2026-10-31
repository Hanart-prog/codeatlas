import type { AnalyzeRequest } from "@/types/generated/AnalyzeRequest";
import type { ApiErrorBody } from "@/types/generated/ApiErrorBody";
import type { CommitPage } from "@/types/generated/CommitPage";
import type { HealthResponse } from "@/types/generated/HealthResponse";
import type { JobResponse } from "@/types/generated/JobResponse";
import type { OverviewResponse } from "@/types/generated/OverviewResponse";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

const BASE_URL = "/api/v1";
const UNREACHABLE = "The local API did not respond.";

export class ApiRequestError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "ApiRequestError";
    this.code = code;
  }
}

/** Job responses have an `error` field too, so only a populated object counts. */
function isErrorBody(value: unknown): value is ApiErrorBody {
  if (typeof value !== "object" || value === null || !("error" in value)) return false;
  const { error } = value;
  return typeof error === "object" && error !== null && "code" in error;
}

async function send(path: string, init: RequestInit): Promise<{ response: Response; body: unknown }> {
  let response: Response;
  try {
    response = await fetch(`${BASE_URL}${path}`, init);
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "AbortError") throw cause;
    throw new ApiRequestError("unreachable", UNREACHABLE);
  }
  const body: unknown = await response.json().catch(() => null);
  if (isErrorBody(body)) {
    throw new ApiRequestError(body.error.code, body.error.message);
  }
  return { response, body };
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const { response, body } = await send(path, init);
  if (!response.ok || body === null) {
    throw new ApiRequestError("unreachable", UNREACHABLE);
  }
  return body as T;
}

function postJson<T>(path: string, payload: unknown): Promise<T> {
  return request<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

/** A degraded server answers 503 with a normal health body, which is data here, not a failure. */
export async function fetchHealth(signal?: AbortSignal): Promise<HealthResponse> {
  const { response, body } = await send("/health", signal ? { signal } : {});
  if (body === null || (!response.ok && response.status !== 503)) {
    throw new ApiRequestError("unreachable", UNREACHABLE);
  }
  return body as HealthResponse;
}

export const fetchRepositories = (signal: AbortSignal) =>
  request<RepositoryEntry[]>("/repositories", { signal });

export const fetchOverview = (id: number, signal: AbortSignal) =>
  request<OverviewResponse>(`/repositories/${id}/overview`, { signal });

export const fetchCommits = (id: number, limit: number, signal: AbortSignal) =>
  request<CommitPage>(`/repositories/${id}/commits?limit=${limit}`, { signal });

export const fetchJob = (id: number) => request<JobResponse>(`/jobs/${id}`);

export const openRepository = (path: string) => postJson<RepositoryEntry>("/repositories/open", { path });

export const startAnalysis = (id: number, options: AnalyzeRequest = {}) =>
  postJson<JobResponse>(`/repositories/${id}/analyze`, options);
