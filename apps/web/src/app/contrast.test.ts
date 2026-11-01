import { readFileSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

const css = readFileSync(path.join(import.meta.dirname, "../index.css"), "utf8");

function tokens(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  const block = css.slice(start, css.indexOf("}", start));
  return Object.fromEntries([...block.matchAll(/--([a-z-]+): (#[0-9a-f]{6});/g)].map((match) => [match[1], match[2]]));
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16) / 255);
  const [red, green, blue] = channels.map((value) =>
    value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
  ) as [number, number, number];
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(one: string, two: string): number {
  const [light, dark] = [luminance(one), luminance(two)].sort((a, b) => b - a) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

const THEMES = { light: tokens(":root"), dark: { ...tokens(":root"), ...tokens(".dark") } };
const TEXT = ["foreground", "muted-foreground", "success", "warning", "destructive"];
const SURFACES = ["background", "card", "muted", "sidebar"];

describe.each(Object.entries(THEMES))("%s theme", (_, theme) => {
  it.each(TEXT.flatMap((text) => SURFACES.map((surface) => [text, surface] as const)))(
    "%s text is readable on %s",
    (text, surface) => {
      expect(contrast(theme[text] as string, theme[surface] as string)).toBeGreaterThanOrEqual(4.5);
    },
  );

  it("buttons are readable", () => {
    expect(contrast(theme["primary-foreground"] as string, theme["primary"] as string)).toBeGreaterThanOrEqual(4.5);
  });
});
