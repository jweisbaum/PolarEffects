import { expect, it } from "vitest";

import THEMES from "./settings/themes.json";
import { contrastRatio, needsOutline } from "./colourContrast";

/** pe-core's DEFAULT_BLEND_COLOUR. */
const DEFAULT_BLEND = "#e0457b";

it("measures contrast as WCAG does", () => {
  expect(contrastRatio("#ffffff", "#000000")).toBeCloseTo(21, 5);
  expect(contrastRatio("#777", "#777777")).toBeCloseTo(1, 5);
  expect(contrastRatio("rgba(37, 52, 71, 0.96)", "#253447")).toBeCloseTo(1, 5);
  expect(contrastRatio("not a colour", "#000")).toBe(21);
});

it("gives new projects a blend colour seen on every bundled theme, and outlines an old white one on Paper", () => {
  for (const theme of THEMES) {
    for (const background of [theme.roles.surface, theme.roles.inset]) {
      expect(needsOutline(DEFAULT_BLEND, background), `${theme.id} ${background}`).toBe(false);
    }
  }
  const paper = THEMES.find((theme) => theme.id === "paper")!;
  const harbour = THEMES.find((theme) => theme.id === "harbour")!;
  expect(needsOutline("#ffffff", paper.roles.surface)).toBe(true);
  expect(needsOutline("#ffffff", paper.roles.inset)).toBe(true);
  expect(needsOutline("#ffffff", harbour.roles.surface)).toBe(false);
});
