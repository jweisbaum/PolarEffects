// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import { TEST_BLEND } from "../testBlend";

const polarPlotCall = vi.fn();
vi.mock("../ipc", async () => {
  const { emptyDots } = await import("./dotPacket");
  return { api: { polarPlot: (tws: number | null) => polarPlotCall(tws), polarPlotDots: async () => emptyDots() } };
});

const { default: PolarPlot } = await import("./PolarPlot");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

const source = (overrides: Partial<SourceSummary> = {}): SourceSummary => ({
  id: 1, kind: "polar_file", label: "A", colour: "#4e79a7", visible: true, weight: 1,
  count: 4, used: null, polar_file: null, orc: null, track: null, edits: 0, ...overrides,
});

const project = (overrides: Partial<ProjectSummary> = {}): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision: 1, boat_name: "", boat_notes: "",
  sources: [source()], can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
  ...overrides,
});

const result = (overrides: Partial<PolarPlotResult> = {}): PolarPlotResult => ({
  tws_min: 6, tws_max: 20, curves: [], blend: [], band_kn: 1, ...overrides,
});

async function render(...args: Parameters<typeof PolarPlot>) {
  await act(async () => root.render(<PolarPlot {...args[0]} />));
  // The fetch effect's `.then` runs a microtask turn after `render` returns;
  // give it one more pass so `setResult` lands before assertions run.
  await act(async () => { await Promise.resolve(); });
}

beforeEach(() => {
  polarPlotCall.mockReset();
  polarPlotCall.mockResolvedValue(result());
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("shows the placeholder and no controls-affecting fetch stall when there is no visible polar source", async () => {
  await render({ project: project({ sources: [] }), variant: "panel" });
  expect(host.querySelector(".plot-placeholder")?.textContent).toBe(
    "The polar plot appears here once the project has a source.",
  );
  expect(host.querySelector("canvas")).toBeNull();
});

it("fetches the plot for the project, defaulting to \"all\" (tws null)", async () => {
  await render({ project: project(), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledWith(null);
  expect(host.querySelector('[data-feature="plot:all"]')).not.toBeNull();
  expect(host.querySelector('[data-feature="plot:tws"]')).not.toBeNull();
});

it("refetches when the project's revision changes, but not on an unrelated rerender with the same revision", async () => {
  await render({ project: project({ revision: 1 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(1);
  await render({ project: project({ revision: 1 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(1);
  await render({ project: project({ revision: 2 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(2);
});

it("switches off \"all\" to a specific wind speed within the fetched domain", async () => {
  await render({ project: project(), variant: "panel" });
  const all = host.querySelector<HTMLInputElement>('[data-feature="plot:all"]')!;
  expect(all.checked).toBe(true);
  await act(async () => all.click());
  const slider = host.querySelector<HTMLInputElement>('[data-feature="plot:tws"]')!;
  expect(slider.disabled).toBe(false);
  const called = polarPlotCall.mock.calls.at(-1)?.[0] as number;
  expect(called).toBeGreaterThanOrEqual(6);
  expect(called).toBeLessThanOrEqual(20);
});

it("shows the full-size button only in the panel variant and the close button only in the overlay variant", async () => {
  await render({ project: project(), variant: "panel" });
  expect(host.querySelector('[data-feature="plot:full-size"]')).not.toBeNull();
  expect(host.querySelector('[data-feature="plot:close"]')).toBeNull();

  await render({ project: project(), variant: "overlay" });
  expect(host.querySelector('[data-feature="plot:full-size"]')).toBeNull();
  expect(host.querySelector('[data-feature="plot:close"]')).not.toBeNull();
});

it("calls onFullSize and onClose", async () => {
  const onFullSize = vi.fn();
  const onClose = vi.fn();
  await render({ project: project(), variant: "panel", onFullSize, onClose });
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="plot:full-size"]')!.click());
  expect(onFullSize).toHaveBeenCalledTimes(1);

  await render({ project: project(), variant: "overlay", onFullSize, onClose });
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="plot:close"]')!.click());
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("shows a message when the plot has no point at the chosen slice", async () => {
  polarPlotCall.mockResolvedValue(result({ curves: [{ source_id: 1, label: "A", colour: "#4e79a7", tws: 10, points: [] }] }));
  await render({ project: project(), variant: "panel" });
  expect(host.querySelector(".polar-plot-empty")?.textContent).toBe("No source has data at this wind speed.");
});

it("sizes the fan and the hover hit-test to the blend too (M6 carry)", async () => {
  const { plotMaxBsp } = await import("./PolarPlot");
  const { emptyDots } = await import("./dotPacket");
  const blend = { source_id: null, label: "Blend", colour: "#ff8800", tws: 10, points: [{ twa: 90, bsp: 12 }] };
  const plotted = result({ curves: [{ source_id: 1, label: "A", colour: "#4e79a7", tws: 10, points: [{ twa: 90, bsp: 8 }] }], blend: [blend] });
  expect(plotMaxBsp(plotted, emptyDots())).toBe(12);
  expect(plotMaxBsp(result({ blend: [] }), emptyDots())).toBe(0);
});

it("counts a blend-only slice as something to draw", async () => {
  polarPlotCall.mockResolvedValue(result({
    blend: [{ source_id: null, label: "Blend", colour: "#ffffff", tws: 10, points: [{ twa: 90, bsp: 7 }] }],
  }));
  await render({ project: project(), variant: "panel" });
  expect(host.querySelector(".polar-plot-empty")).toBeNull();
});
