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

test("the content security policy allows the app to run", async ({ page }) => {
  const response = await page.goto("/");
  expect(response?.headers()["content-security-policy"]).toContain("default-src 'self'");
  await expect(page.getByRole("heading", { name: "Overview" })).toBeVisible();
});
