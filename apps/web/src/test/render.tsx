import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, type RenderResult } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";

import type { HealthResponse } from "@/types/generated/HealthResponse";

export const healthyBody: HealthResponse = {
  status: "ok",
  version: "0.1.0",
  api_version: "v1",
  parser_version: 0,
  algorithm_version: 0,
  database: { reachable: true, schema_version: 1, expected_schema_version: 1 },
};

type Route = { status?: number; body: unknown };

/** Answers by path suffix; `/repositories` lists nothing unless a route says otherwise. */
export function stubRoutes(routes: Record<string, Route>) {
  const table: Record<string, Route> = { "/repositories": { body: [] }, ...routes };
  const fetchMock = vi.fn((...[url]: [url: string, init?: RequestInit]) => {
    const path = url.replace("/api/v1", "").split("?")[0] ?? "";
    const route = table[path];
    if (!route) {
      const error = { error: { code: "not_found", message: `no stub for ${path}`, details: null } };
      return Promise.resolve(new Response(JSON.stringify(error), { status: 404 }));
    }
    const text = typeof route.body === "string" ? route.body : JSON.stringify(route.body);
    return Promise.resolve(new Response(text, { status: route.status ?? 200 }));
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

export function stubFetch(status: number, body: unknown) {
  return stubRoutes({ "/health": { status, body } });
}

export function renderWithQuery(element: ReactElement): RenderResult {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{element}</QueryClientProvider>);
}
