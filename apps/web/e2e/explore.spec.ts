import { readFileSync } from "node:fs";
import path from "node:path";

import { expect, test } from "@playwright/test";

const fixturePath = (name: string) => readFileSync(path.join(import.meta.dirname, name), "utf8");
const fixture = () => fixturePath(".fixture-path");
const scaleFixture = () => fixturePath(".scale-fixture-path");
const slowFixture = () => fixturePath(".slow-fixture-path");

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

test("write a report and see what it holds", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(fixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("cell", { name: "src/billing/invoice.ts" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("list", { name: "Recent jobs" }).getByText("Finished").first()).toBeVisible();

  await page.getByRole("link", { name: /Reports/ }).click();
  await expect(page.getByText(/13 first-parent commits/)).toBeVisible();

  const download = page.waitForEvent("download");
  await page.getByRole("link", { name: "Download HTML" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("codeatlas-report.html");
  const html = readFileSync(await file.path(), "utf8");
  expect(html).toContain("src/billing/invoice.ts");
  expect(html).not.toContain("<script");

  // The report must also work on its own, with no server and no scripts.
  await page.setContent(html);
  await expect(page.getByRole("heading", { name: "Provenance" })).toBeVisible();

  expect(errors).toEqual([]);
});

test("a very large repository opens on a map and stays responsive", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(scaleFixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByText("3,000", { exact: false }).first()).toBeVisible({ timeout: 60_000 });

  await page.getByRole("link", { name: /Architecture Explorer/ }).click();
  await expect(page.getByText(/groups of 3,000 files/)).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("canvas").first()).toBeVisible();

  // The drawn map must be small, and the page must still answer.
  const started = Date.now();
  await page.getByRole("list", { name: "Groups" }).getByRole("button").first().click();
  await expect(page.getByRole("navigation", { name: "Directory" }).getByRole("button", { name: "/ pkg" }).or(
    page.getByRole("navigation", { name: "Directory" }).getByRole("button", { name: /pkg00/ }),
  ).first()).toBeVisible({ timeout: 15_000 });
  expect(Date.now() - started).toBeLessThan(15_000);

  // Search covers the opened directory, so go back up to reach any file.
  await page.getByRole("navigation", { name: "Directory" }).getByRole("button", { name: "root" }).click();
  await page.getByLabel("Search files").fill("file2999.ts");
  await expect(page.getByRole("button", { name: /file2999\.ts$/ })).toBeVisible({ timeout: 15_000 });

  expect(errors).toEqual([]);
});

test("cancel a running analysis and carry on", async ({ page }) => {
  await page.goto("/");
  await page.getByLabel(/Absolute path/).fill(slowFixture());
  await page.getByRole("button", { name: "Analyze" }).click();
  await page.getByRole("button", { name: "Cancel" }).first().click();

  // A fast machine can finish first, which is fine; a job must never hang.
  await expect(page.getByRole("button", { name: "Analyze" })).toBeEnabled({ timeout: 30_000 });
  await page.getByRole("button", { name: "Analyze" }).click();
  await expect(page.getByRole("button", { name: "Analyze" })).toBeEnabled({ timeout: 60_000 });
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByText("First-parent commits")).toBeVisible();
});

test("the benchmarks page names every measured repository", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("link", { name: /Benchmarks/ }).click();
  for (const name of ["generated demo", "sindresorhus/ky", "colinhacks/zod", "nestjs/nest"]) {
    await expect(page.getByText(name, { exact: true })).toBeVisible();
  }
  await expect(page.getByText("Public project, MIT").first()).toBeVisible();
  await expect(page.getByText("Generated by CodeAtlas").first()).toBeVisible();
});
