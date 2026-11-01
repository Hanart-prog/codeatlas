import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { healthyBody, renderWithQuery, stubFetch, stubRoutes } from "@/test/render";

import { App } from "./App";

describe("App", () => {
  it("starts on the overview and reports the API connection", async () => {
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    expect(screen.getByRole("link", { name: "Overview" })).toHaveAttribute("aria-current", "page");
    expect(await screen.findByText("API connected")).toBeInTheDocument();
  });

  it("links to every dashboard section from the specification", () => {
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    const navigation = within(screen.getByRole("navigation", { name: "Primary" }));
    for (const label of ["Overview", "Architecture Explorer", "Change Impact", "Time Machine", "Backtesting", "Reports"]) {
      expect(navigation.getByRole("link", { name: new RegExp(label) })).toBeInTheDocument();
    }
  });

  it("opens every section without a repository", async () => {
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    await userEvent.click(screen.getByRole("link", { name: /Reports/ }));
    expect(await screen.findByRole("heading", { name: "Reports" })).toBeInTheDocument();
    expect(screen.getByText("Open a repository from the Overview first.")).toBeInTheDocument();
  });

  it("says how to start the API when it is down", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("connection refused"))));
    renderWithQuery(<App />);
    expect(await screen.findByText(/local API is not running/)).toBeInTheDocument();
    expect(screen.getByText("make dev")).toBeInTheDocument();
  });

  it("does not select a repository whose analysis never finished", async () => {
    localStorage.removeItem("codeatlas-repository");
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories": { body: [{ id: 9, path: "/work/empty", analyzed_at: null }] },
    });
    renderWithQuery(<App />);
    expect(await screen.findByText("No repository open")).toBeInTheDocument();
  });

  it("marks the generated demo so its results are not read as real", async () => {
    stubRoutes({
      "/health": { body: healthyBody },
      "/repositories": { body: [{ id: 1, path: "/cache/demo/product", analyzed_at: 1 }] },
    });
    renderWithQuery(<App />);
    expect(await screen.findByRole("note")).toHaveTextContent("say nothing about real projects");
  });

  it("falls back to the overview for unknown routes", () => {
    window.location.hash = "#/nope";
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    expect(screen.getByRole("heading", { name: "Overview" })).toBeInTheDocument();
  });

  it("toggles and remembers the theme", async () => {
    stubFetch(200, healthyBody);
    localStorage.removeItem("codeatlas-theme");
    renderWithQuery(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
    expect(localStorage.getItem("codeatlas-theme")).toBe("dark");
  });
});
