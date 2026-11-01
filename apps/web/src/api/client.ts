import type { BacktestRequest } from "@/types/generated/BacktestRequest";
import type { TimelineResponse } from "@/types/generated/TimelineResponse";
import type { CompareRequest } from "@/types/generated/CompareRequest";
import type { CompareResponse } from "@/types/generated/CompareResponse";
import type { AnalyzeRequest } from "@/types/generated/AnalyzeRequest";
import type { ApiErrorBody } from "@/types/generated/ApiErrorBody";
import type { CommitPage } from "@/types/generated/CommitPage";
import type { DirectoryMap } from "@/types/generated/DirectoryMap";
import type { FileDetail } from "@/types/generated/FileDetail";
import type { GraphResponse } from "@/types/generated/GraphResponse";
import type { HealthResponse } from "@/types/generated/HealthResponse";
import type { ImpactRequest } from "@/types/generated/ImpactRequest";
import type { ImpactResponse } from "@/types/generated/ImpactResponse";
import type { LatestBacktest } from "@/types/generated/LatestBacktest";
import type { JobResponse } from "@/types/generated/JobResponse";
import type { OverviewResponse } from "@/types/generated/OverviewResponse";
import type { ReportDocument } from "@/types/generated/ReportDocument";
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

export interface GraphFilter {
  ref?: string | undefined;
  directory?: string | undefined;
  search?: string | undefined;
  limit?: number | undefined;
  include?: "cochange" | undefined;
}

function query(values: Record<string, string | undefined>): string {
  const entries = Object.entries(values).filter((entry): entry is [string, string] => Boolean(entry[1]));
  return new URLSearchParams(entries).toString();
}

export const fetchGraph = (id: number, filter: GraphFilter, signal: AbortSignal) =>
  request<GraphResponse>(
    `/repositories/${id}/graph?${query({ ...filter, limit: filter.limit?.toString() })}`,
    { signal },
  );

export const fetchDirectoryMap = (id: number, ref: string | undefined, directory: string, signal: AbortSignal) =>
  request<DirectoryMap>(`/repositories/${id}/graph/directories?${query({ ref, directory })}`, { signal });

export const fetchReport = (id: number, signal: AbortSignal) =>
  request<ReportDocument>(`/repositories/${id}/report`, { signal });

export const reportUrl = (id: number, format: "html" | "json") =>
  `${BASE_URL}/repositories/${id}/report?format=${format}`;

export const fetchFileDetail = (id: number, path: string, ref: string | undefined, signal: AbortSignal) =>
  request<FileDetail>(`/repositories/${id}/files?${query({ path, ref })}`, { signal });

export const runImpact = (id: number, impact: ImpactRequest) =>
  postJson<ImpactResponse>(`/repositories/${id}/impact`, impact);

export const runCompare = (id: number, compare: CompareRequest) =>
  postJson<CompareResponse>(`/repositories/${id}/compare`, compare);

export const fetchTimeline = (id: number, signal: AbortSignal) =>
  request<TimelineResponse>(`/repositories/${id}/timeline`, { signal });

export const fetchBacktest = (id: number, signal: AbortSignal) =>
  request<LatestBacktest>(`/repositories/${id}/backtests`, { signal });

export const startBacktest = (id: number, backtest: BacktestRequest) =>
  postJson<JobResponse>(`/repositories/${id}/backtests`, backtest);

export const fetchJob = (id: number) => request<JobResponse>(`/jobs/${id}`);

export const cancelJob = (id: number) => postJson<JobResponse>(`/jobs/${id}/cancel`, {});

export const fetchRepositoryJobs = (id: number, signal: AbortSignal) =>
  request<JobResponse[]>(`/repositories/${id}/jobs`, { signal });

export const openRepository = (path: string) => postJson<RepositoryEntry>("/repositories/open", { path });

export const startAnalysis = (id: number, options: AnalyzeRequest = {}) =>
  postJson<JobResponse>(`/repositories/${id}/analyze`, options);
