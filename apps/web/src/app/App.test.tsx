import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { healthyBody, renderWithQuery, stubFetch } from "@/test/render";

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

  it("says plainly when a section is not built", async () => {
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    await userEvent.click(screen.getByRole("link", { name: /Change Impact/ }));
    expect(await screen.findByRole("heading", { name: "Change Impact" })).toBeInTheDocument();
    expect(screen.getByText(/planned for Phase 3/)).toBeInTheDocument();
  });

  it("falls back to the overview for unknown routes", () => {
    window.location.hash = "#/nope";
    stubFetch(200, healthyBody);
    renderWithQuery(<App />);
    expect(screen.getByRole("heading", { name: "Overview" })).toBeInTheDocument();
  });

  it("toggles and remembers the theme", async () => {
    stubFetch(200, healthyBody);
    document.documentElement.classList.add("dark");
    renderWithQuery(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    await waitFor(() => expect(document.documentElement).not.toHaveClass("dark"));
    expect(localStorage.getItem("codeatlas-theme")).toBe("light");
  });
});
