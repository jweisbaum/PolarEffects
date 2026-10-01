// @vitest-environment happy-dom
/**
 * What hovering the blend shows (spec.md 9.2, 10.1): the cell, where its
 * value came from, and each source behind it with its speed and its share.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it } from "vitest";

import type { BlendCell } from "../generated/BlendCell";
import type { SourceSummary } from "../generated/SourceSummary";
import BlendCellTooltip from "./BlendCellTooltip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
beforeEach(() => { host = document.createElement("div"); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

const source = (id: number, label: string, colour: string): SourceSummary => ({
  id, kind: "polar_file", label, colour, visible: true, weight: 1,
  count: 4, used: null, polar_file: null, orr: null, orc: null, track: null, edits: 0,
});
const sources = [source(1, "Farr 40", "#4e79a7"), source(2, "Fastnet 2025", "#e15759")];

const cell = (overrides: Partial<BlendCell> = {}): BlendCell => ({
  twa: 90, tws: 12, bsp: 7.524, origin: "direct", corrected: false,
  contributors: [
    { source_id: 1, bsp: 8, weight: 0.5, share: 0.6 },
    { source_id: 2, bsp: 6.81, weight: 1 / 3, share: 0.4 },
  ],
  ...overrides,
});

async function show(value: BlendCell, extra: { unit?: "kn" | "ms" | "kmh"; smoothing?: boolean } = {}) {
  await act(async () => root.render(
    <BlendCellTooltip cell={value} sources={sources} colour="#e0457b" unit={extra.unit ?? "kn"}
      smoothing={extra.smoothing ?? false} x={10} y={20} />,
  ));
  return host.querySelector<HTMLElement>('[role="tooltip"]')!;
}
const rows = (tip: HTMLElement) => [...tip.querySelectorAll(".blend-cell-sources li")].map((row) => row.textContent);

it("names the cell, its value and each source with its speed and share", async () => {
  const tip = await show(cell());
  expect(tip.querySelector("strong")?.textContent).toBe("Blend");
  expect(tip.querySelector("dl")?.textContent).toBe("TWA90°TWS12.0 knBSP7.52 kn");
  expect(rows(tip)).toEqual(["Farr 408.00 kn60%", "Fastnet 20256.81 kn40%"]);
  expect(tip.querySelector(".blend-cell-note")).toBeNull();
  expect(tip.style.left).toBe("10px");
});

it("shows speeds in the display unit", async () => {
  const tip = await show(cell(), { unit: "ms" });
  // 7.524 kn × 0.514444 = 3.87 m/s; 8 kn = 4.12 m/s.
  expect(tip.querySelector("dl")?.textContent).toContain("BSP3.87 m/s");
  expect(rows(tip)[0]).toBe("Farr 404.12 m/s60%");
});

it("says a filled cell was interpolated and an empty one is not reached", async () => {
  let tip = await show(cell({ origin: "filled", contributors: [], bsp: 7.1 }));
  expect(tip.querySelector(".blend-cell-note")?.textContent).toBe("Filled in between neighbouring cells; no source has a value here.");
  expect(tip.querySelector(".blend-cell-sources")).toBeNull();
  tip = await show(cell({ origin: "empty", contributors: [], bsp: null }));
  expect(tip.querySelector("dl")?.textContent).toContain("BSP–");
  expect(tip.querySelector(".blend-cell-note")?.textContent).toBe("No source reaches this cell.");
});

it("says the zero-degree row is zero by definition", async () => {
  const tip = await show(cell({ twa: 0, origin: "filled", contributors: [], bsp: 0 }));
  expect(tip.querySelector(".blend-cell-note")?.textContent).toBe("Head to wind: 0 by definition.");
});

it("says when the value is a manual correction or was smoothed, and still lists the sources", async () => {
  let tip = await show(cell({ corrected: true, bsp: 9.25 }));
  expect(tip.querySelector(".blend-cell-note")?.textContent).toBe("Corrected by hand; the sources below gave the value before.");
  expect(rows(tip)).toHaveLength(2);
  tip = await show(cell(), { smoothing: true });
  expect(tip.querySelector(".blend-cell-note")?.textContent).toBe("Smoothed after blending these sources.");
});

it("names a source the summary no longer holds as unknown", async () => {
  const tip = await show(cell({ contributors: [{ source_id: 99, bsp: 5, weight: 1, share: 1 }] }));
  expect(rows(tip)).toEqual(["Unknown source5.00 kn100%"]);
});
