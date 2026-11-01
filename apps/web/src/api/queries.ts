import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { BacktestRequest } from "@/types/generated/BacktestRequest";
import type { CompareRequest } from "@/types/generated/CompareRequest";
import type { ImpactRequest } from "@/types/generated/ImpactRequest";
import type { JobResponse } from "@/types/generated/JobResponse";

import {
  ApiRequestError,
  type GraphFilter,
  fetchBacktest,
  fetchCommits,
  fetchFileDetail,
  fetchGraph,
  fetchJob,
  fetchOverview,
  fetchRepositories,
  fetchTimeline,
  openRepository,
  runCompare,
  runImpact,
  startAnalysis,
  startBacktest,
} from "./client";

const JOB_POLL_MS = 300;

export function useRepositories() {
  return useQuery({
    queryKey: ["repositories"],
    queryFn: ({ signal }) => fetchRepositories(signal),
    retry: false,
  });
}

export function useOverview(id: number | undefined) {
  return useQuery({
    queryKey: ["overview", id],
    queryFn: ({ signal }) => fetchOverview(id as number, signal),
    enabled: id !== undefined,
    retry: false,
  });
}

export function useCommits(id: number | undefined, limit: number, enabled: boolean) {
  return useQuery({
    queryKey: ["commits", id, limit],
    queryFn: ({ signal }) => fetchCommits(id as number, limit, signal),
    enabled: enabled && id !== undefined,
    retry: false,
  });
}

export function useGraph(id: number | undefined, filter: GraphFilter) {
  return useQuery({
    queryKey: ["graph", id, filter],
    queryFn: ({ signal }) => fetchGraph(id as number, filter, signal),
    enabled: id !== undefined,
    retry: false,
    placeholderData: keepPreviousData,
  });
}

export function useImpact() {
  return useMutation({
    mutationFn: ({ id, request }: { id: number; request: ImpactRequest }) => runImpact(id, request),
  });
}

export function useTimeline(id: number | undefined) {
  return useQuery({
    queryKey: ["timeline", id],
    queryFn: ({ signal }) => fetchTimeline(id as number, signal),
    enabled: id !== undefined,
    retry: false,
  });
}

export function useCompare() {
  return useMutation({
    mutationFn: ({ id, request }: { id: number; request: CompareRequest }) => runCompare(id, request),
  });
}

export function useFileDetail(id: number | undefined, path: string | undefined, ref: string | undefined) {
  return useQuery({
    queryKey: ["file", id, path, ref],
    queryFn: ({ signal }) => fetchFileDetail(id as number, path as string, ref, signal),
    enabled: id !== undefined && path !== undefined,
    retry: false,
  });
}

async function waitForJob(job: JobResponse, onUpdate?: (job: JobResponse) => void): Promise<void> {
  let current = job;
  while (current.status === "queued" || current.status === "running") {
    await new Promise((resolve) => setTimeout(resolve, JOB_POLL_MS));
    current = await fetchJob(current.id);
    onUpdate?.(current);
  }
  if (current.status === "failed") {
    throw new ApiRequestError("analysis_failed", current.error ?? "The analysis failed.");
  }
}

export function useLatestBacktest(id: number | undefined) {
  return useQuery({
    queryKey: ["backtest", id],
    queryFn: ({ signal }) => fetchBacktest(id as number, signal),
    enabled: id !== undefined,
    retry: false,
  });
}

/** Runs a backtest and waits for it, reporting each poll. */
export function useRunBacktest(onUpdate: (job: JobResponse) => void) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, request }: { id: number; request: BacktestRequest }) => {
      await waitForJob(await startBacktest(id, request), onUpdate);
    },
    onSuccess: (_, { id }) => client.invalidateQueries({ queryKey: ["backtest", id] }),
  });
}

/** Opens a repository by path, analyzes it and waits for the result. */
export function useAnalyzeRepository(onDone: (id: number) => void) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (path: string) => {
      const repository = await openRepository(path);
      await waitForJob(await startAnalysis(repository.id));
      return repository.id;
    },
    onSuccess: async (id) => {
      onDone(id);
      await client.invalidateQueries();
    },
    onSettled: () => client.invalidateQueries({ queryKey: ["repositories"] }),
  });
}
