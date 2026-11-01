import { readFileSync } from "node:fs";
import path from "node:path";

import { expect, test } from "@playwright/test";

const fixture = () => readFileSync(path.join(import.meta.dirname, ".fixture-path"), "utf8");

test("analyze a repository and explore its graph at two commits", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(fixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("cell", { name: "src/billing/invoice.ts" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("link", { name: /Architecture Explorer/ }).click();
  await expect(page.getByText("8 of 8 files")).toBeVisible();
  await expect(page.getByText("No import cycles at this commit.")).toBeVisible();
  await expect(page.locator("canvas").first()).toBeVisible();

  const cycleCommit = page.getByRole("option", { name: /Introduce session and token cycle/ });
  const value = await cycleCommit.getAttribute("value");
  await page.getByLabel("Commit").selectOption(value as string);
  await expect(page.getByText("6 of 6 files")).toBeVisible();
  await expect(page.getByText("2 files", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "src/auth/token.ts" }).click();
  await expect(page.getByText("in an import cycle")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Imports (1)" })).toBeVisible();

  expect(errors).toEqual([]);
});

test("rank impact as of a historical commit", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(fixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("cell", { name: "src/billing/invoice.ts" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("link", { name: /Change Impact/ }).click();
  await page.getByLabel("Add a file").fill("src/auth/session.ts");
  await page.getByRole("button", { name: "Add" }).click();
  await page.getByRole("button", { name: "Rank files" }).click();

  const first = page.getByRole("listitem").filter({ hasText: "src/billing/invoice.ts" });
  await expect(first.getByText("0.727")).toBeVisible();
  await expect(first.getByText("3 of 4 commits")).toBeVisible();
  await expect(page.getByText("not a probability of failure")).toBeVisible();

  const earlier = page.getByRole("option", { name: /Prorate expired sessions/ });
  await page.getByLabel("As of commit").selectOption((await earlier.getAttribute("value")) as string);
  await page.getByRole("button", { name: "Rank files" }).click();
  await expect(page.getByRole("listitem").filter({ hasText: "src/billing/invoice.ts" }).getByText("0.738")).toBeVisible();

  expect(errors).toEqual([]);
});

test("compare two commits and see a cycle appear", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(fixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("cell", { name: "src/billing/invoice.ts" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("link", { name: /Time Machine/ }).click();
  const commit = async (subject: RegExp) =>
    (await page.getByLabel("Before").getByRole("option", { name: subject }).getAttribute("value")) as string;

  await page.getByLabel("Before").selectOption(await commit(/Add billing/));
  await page.getByLabel("After").selectOption(await commit(/Introduce session and token cycle/));
  await page.getByRole("button", { name: "Compare" }).click();

  await expect(page.getByText("New cycles (1)")).toBeVisible();
  await expect(page.getByText("Added imports (2)")).toBeVisible();
  await expect(page.getByText("Renamed files (1)")).toBeVisible();
  await expect(page.locator("canvas").first()).toBeVisible();
  await expect(page.getByRole("row", { name: /Imports between them/ }).getByText("+2")).toBeVisible();

  await page.getByRole("button", { name: "src/auth/session.ts" }).first().click();
  await expect(page.getByText("3 → 4").first()).toBeVisible();

  await page.getByLabel("Before").selectOption(await commit(/Introduce session and token cycle/));
  await page.getByLabel("After").selectOption(await commit(/Break session and token cycle/));
  await page.getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText("Resolved cycles (1)")).toBeVisible();
  await expect(page.getByText("Removed imports (2)")).toBeVisible();

  await expect(page.getByText("13 of 13 commits")).toBeVisible();
  await expect(page.getByRole("img", { name: /^Import cycles by commit/ })).toBeVisible();

  expect(errors).toEqual([]);
});

test("backtest against baselines without look-ahead", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(fixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("cell", { name: "src/billing/invoice.ts" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("link", { name: /Backtesting/ }).click();
  await expect(page.getByText(/does not measure bugs/)).toBeVisible();

  await page.getByRole("button", { name: "Run backtest" }).click();
  await expect(page.getByText(/No commit could be evaluated/)).toBeVisible();

  await page.getByLabel("Warm-up commits").fill("4");
  await page.getByRole("button", { name: "Run backtest" }).click();
  await expect(page.getByRole("heading", { name: "Against the baselines" }).or(page.getByText("Against the baselines"))).toBeVisible();
  await expect(page.getByRole("rowheader", { name: "Most changed files" })).toBeVisible();
  await expect(page.getByRole("img", { name: /Hit rate at 5 files for each ranker/ })).toBeVisible();

  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export JSON" }).click();
  expect((await download).suggestedFilename()).toMatch(/^codeatlas-backtest-.*\.json$/);

  expect(errors).toEqual([]);
});

test("the content security policy allows the app to run", async ({ page }) => {
  const response = await page.goto("/");
  expect(response?.headers()["content-security-policy"]).toContain("default-src 'self'");
  await expect(page.getByRole("heading", { name: "Overview" })).toBeVisible();
});
