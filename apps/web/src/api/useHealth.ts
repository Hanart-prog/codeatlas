import { useQuery } from "@tanstack/react-query";

import { fetchHealth } from "./client";

export const HEALTH_POLL_MS = 10_000;

export function useHealth() {
  return useQuery({
    queryKey: ["health"],
    queryFn: ({ signal }) => fetchHealth(signal),
    refetchInterval: HEALTH_POLL_MS,
    retry: false,
  });
}
