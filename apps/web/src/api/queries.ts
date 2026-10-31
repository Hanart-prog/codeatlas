import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { JobResponse } from "@/types/generated/JobResponse";

import {
  ApiRequestError,
  fetchCommits,
  fetchJob,
  fetchOverview,
  fetchRepositories,
  openRepository,
  startAnalysis,
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

async function waitForJob(job: JobResponse): Promise<void> {
  let current = job;
  while (current.status === "queued" || current.status === "running") {
    await new Promise((resolve) => setTimeout(resolve, JOB_POLL_MS));
    current = await fetchJob(current.id);
  }
  if (current.status === "failed") {
    throw new ApiRequestError("analysis_failed", current.error ?? "The analysis failed.");
  }
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
