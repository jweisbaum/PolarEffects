// @vitest-environment happy-dom
/**
 * The 3D stage against a stand-in scene (happy-dom has no WebGL) and a
 * mocked backend: it fetches the packed scene on every revision, selects by
 * click, excludes through Rust, disposes the scene on unmount, and still
 * shows its controls where WebGL is missing.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import { FLAG_EXCLUDED, FLAG_FILTERED, type ScenePacket } from "./scenePacket";

const scenes = vi.hoisted(() => ({ made: [] as FakeScene[], fail: false, pick: -1 }));

interface FakeScene {
  setData: ReturnType<typeof vi.fn>;
  setSelection: ReturnType<typeof vi.fn>;
  dispose: ReturnType<typeof vi.fn>;
}

vi.mock("./scene3d", async (original) => {
  const actual = await original<typeof import("./scene3d")>();
  class PolarScene {
    camera = { fov: 40 };
    setData = vi.fn();
    setSelection = vi.fn(() => 0);
    dispose = vi.fn();
    constructor() {
      if (scenes.fail) throw new Error("no WebGL");
      scenes.made.push(this);
    }
    setBackground() {}
    enableControls() {}
    setRotateEnabled() {}
    resize() {}
    setGuides() {}
    setView() {}
    render() {}
    toScreen() { return [0, 0]; }
    pick() { return scenes.pick; }
    lasso() { return new Uint32Array(0); }
    box() { return new Uint32Array(0); }
  }
  return { ...actual, PolarScene };
});

const api = vi.hoisted(() => ({
  polarScene: vi.fn(), setExcluded: vi.fn(), editPolar: vi.fn(), polarEditSurface: vi.fn(), setSegmentStatistic: vi.fn(),
}));
vi.mock("../ipc", () => ({ api }));

const { default: PolarView, draggedSpeed } = await import("./PolarView");
const { editSource } = await import("./editFocus");
const selection = await import("../selection");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

function packet(excluded = false): ScenePacket {
  return {
    timeOrigin: 0,
    samplesKey: 0,
    sources: [{ id: 10, colour: "#ff0000", kind: "orc" }],
    nodes: {
      count: 2, points: Float32Array.from([52, 6, 6, 90, 12, 8]), source: Uint32Array.from([0, 0]),
      cell: Uint32Array.from([0, 1 | (1 << 16)]), flags: Uint32Array.from([0, excluded ? FLAG_EXCLUDED : 0]),
    },
    samples: {
      count: 0, points: new Float32Array(0), source: new Uint32Array(0), ids: new Uint32Array(0),
      hs: new Float32Array(0), current: new Float32Array(0), time: new Float32Array(0), flags: new Uint32Array(0),
    },
    surfaces: [{ source: 0, twa: Float32Array.from([52, 90]), tws: Float32Array.from([6, 12]), bsp: Float32Array.from([6, 7, 7, 8]) }],
  };
}

const project = (revision: number): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision, boat_name: "", boat_notes: "",
  sources: [{ id: 10, kind: "orc", label: "Farr 40", colour: "#ff0000", visible: true, weight: 1, count: 2, used: null, polar_file: null, orc: null, track: null, edits: 0 }],
  can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false,
});

const onProject = vi.fn();

async function render(revision = 1, id = 1) {
  await act(async () => root.render(<PolarView project={{ ...project(revision), id }} settings={null} onProject={onProject} />));
  await act(async () => { await Promise.resolve(); });
}

const q = (selector: string) => host.querySelector<HTMLElement>(selector);
const feature = (id: string) => q(`[data-feature="${id}"]`) as HTMLButtonElement | HTMLSelectElement | null;

async function clickCanvas(shift = false) {
  const canvas = q("canvas")!;
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: 50, clientY: 50, shiftKey: shift }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX: 50, clientY: 50, shiftKey: shift }));
  });
}

/** The same scene with two track samples (ids 5 and 2^32 + 6), the second filtered out. */
function withSamples(): ScenePacket {
  const base = packet();
  return {
    ...base,
    sources: [...base.sources, { id: 30, colour: "#0000ff", kind: "track" }],
    samples: {
      count: 2, points: Float32Array.from([120, 14, 9, 150, 16, 10]), source: Uint32Array.from([1, 1]),
      ids: Uint32Array.from([5, 0, 6, 1]), hs: Float32Array.from([Number.NaN, Number.NaN]),
      current: Float32Array.from([Number.NaN, Number.NaN]), time: Float32Array.from([0, 600]),
      flags: Uint32Array.from([0, FLAG_FILTERED]),
    },
  };
}

beforeEach(() => {
  selection.resetSelection();
  editSource(null);
  api.editPolar.mockReset().mockResolvedValue(project(3));
  api.polarEditSurface.mockReset().mockResolvedValue({
    source_id: 10, kind: "orc", twa: [52, 90], tws: [6, 12], source: [[6, 7], [7, 8]], bsp: [[6, 7], [7, 8]],
    edited: [[false, false], [false, false]], excluded: [[false, false], [false, false]], count: null, spread: null,
    statistic: null, min_samples: 5, edit_count: 0,
  });
  scenes.made = [];
  scenes.fail = false;
  scenes.pick = -1;
  api.polarScene.mockReset().mockResolvedValue(packet());
  api.setExcluded.mockReset().mockResolvedValue(project(2));
  onProject.mockReset();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("fetches the scene on mount and again on every revision, and draws its dots", async () => {
  await render(1);
  expect(api.polarScene).toHaveBeenCalledTimes(1);
  const scene = scenes.made[0]!;
  const last = scene.setData.mock.calls.at(-1)![0];
  expect(last.samples.length).toBe(6);
  expect(last.surfaces).toHaveLength(1);
  await render(2);
  expect(api.polarScene).toHaveBeenCalledTimes(2);
});

it("selects the clicked dot, summarises it and excludes it through Rust", async () => {
  await render();
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
  expect((feature("view3d:exclude") as HTMLButtonElement).disabled).toBe(true);
  scenes.pick = 1;
  await clickCanvas();
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect(q(".view3d-selection")!.textContent).toContain("Mean TWA 90°, TWS 12.0 kn, BSP 8.0 kn");
  expect(q(".view3d-breakdown")!.textContent).toBe("Farr 40: 1");
  expect(scenes.made[0]!.setSelection.mock.calls.at(-1)![0]).toEqual([1]);
  expect((feature("view3d:include") as HTMLButtonElement).disabled).toBe(true);
  expect((feature("view3d:show-on-map") as HTMLButtonElement).disabled).toBe(true);

  await act(async () => (feature("view3d:exclude") as HTMLButtonElement).click());
  expect(api.setExcluded).toHaveBeenCalledWith([{ source_id: 10, twa_index: 1, tws_index: 1 }], [], true);
  expect(onProject).toHaveBeenCalledWith(project(2));

  // The refetched scene has the node excluded; the selection survives it and
  // Include becomes possible.
  api.polarScene.mockResolvedValue(packet(true));
  await render(2);
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect((feature("view3d:include") as HTMLButtonElement).disabled).toBe(false);
  expect((feature("view3d:exclude") as HTMLButtonElement).disabled).toBe(true);
});

it("adds with Shift, clears on an empty click and on Escape", async () => {
  await render();
  scenes.pick = 0;
  await clickCanvas();
  scenes.pick = 1;
  await clickCanvas(true);
  expect(q(".view3d-selection h3")!.textContent).toBe("2 selected");
  await act(async () => q(".view3d")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
  await clickCanvas();
  scenes.pick = -1;
  await clickCanvas();
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
});

it("offers colour modes only when samples have the data", async () => {
  await render();
  const options = [...(feature("view3d:colour") as HTMLSelectElement).options];
  expect(options.map((o) => [o.value, o.disabled])).toEqual([["source", false], ["hs", true], ["current", true], ["time", true]]);
  expect((feature("view3d:show-filtered") as unknown as HTMLInputElement).disabled).toBe(true);
});

it("disposes the scene on unmount", async () => {
  await render();
  const scene = scenes.made[0]!;
  await act(async () => root.unmount());
  expect(scene.dispose).toHaveBeenCalledTimes(1);
  root = createRoot(host);
});

it("says so where WebGL is missing, and keeps its controls", async () => {
  scenes.fail = true;
  await render();
  expect(q(".view3d-unavailable")!.textContent).toBe("The 3D view needs WebGL, which this system does not offer.");
  expect(feature("view3d:layout")).not.toBeNull();
  expect(feature("view3d:exclude")).not.toBeNull();
});

it("shares its selected samples with the map, and shows them there", async () => {
  api.polarScene.mockResolvedValue(withSamples());
  const focused = vi.fn();
  const off = selection.onFocusMap(focused);
  await render();
  // Dot 2 is the first sample (the two nodes come first).
  scenes.pick = 2;
  await clickCanvas();
  expect([...selection.getSampleSelection().ids]).toEqual([5]);
  const show = feature("view3d:show-on-map") as HTMLButtonElement;
  expect(show.disabled).toBe(false);
  await act(async () => show.click());
  expect(focused).toHaveBeenCalledWith({ kind: "selection" });
  off();
});

it("selects what a box on the map selected, but counts and excludes only drawn dots", async () => {
  api.polarScene.mockResolvedValue(withSamples());
  await render();
  await act(async () => selection.selectSamples([5, 2 ** 32 + 6], "map"));
  // The second sample is filtered out and hidden: it is not counted.
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect(q(".view3d-breakdown")!.textContent).toBe("Unknown source: 1");
  await act(async () => (feature("view3d:exclude") as HTMLButtonElement).click());
  expect(api.setExcluded).toHaveBeenCalledWith([], [5], true);
});

it("turns a drag into a boat speed along the BSP axis on screen, snapped with Shift", () => {
  // One knot is 30 px to the right: 45 px right is 1.5 kn more.
  expect(draggedSpeed(6, [30, 0], 45, 10, false)).toBe(7.5);
  // Seen end-on, the axis falls back to 20 px per knot upward.
  expect(draggedSpeed(6, [0, 1], 0, -21, false)).toBe(7.05);
  expect(draggedSpeed(6, [0, 1], 0, -21.5, true)).toBe(7.1);
  expect(draggedSpeed(6, [0, -20], 0, 400, false)).toBe(0);
  expect(draggedSpeed(6, [0, -20], 0, -4000, false)).toBe(60);
});

it("edits the focused source: opaque, with its table, and a drag is one gesture sent to Rust", async () => {
  editSource(10);
  await render();
  expect(api.polarScene).toHaveBeenLastCalledWith(10, null);
  const scene = scenes.made[0]!;
  expect(scene.setData.mock.calls.at(-1)![0].surfaces[0].opaque).toBe(true);
  expect(api.polarEditSurface).toHaveBeenCalledWith(10);
  expect(q(".view3d-edit")!.textContent).toContain("Editing Farr 40");

  await act(async () => (feature("view3d:tool-drag") as HTMLButtonElement).click());
  scenes.pick = 1;
  const canvas = q("canvas")!;
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: 50, clientY: 50 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, button: 0, clientX: 50, clientY: 10 }));
  });
  await act(async () => { await Promise.resolve(); });
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, button: 0, clientX: 50, clientY: 31, shiftKey: true }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX: 50, clientY: 31 }));
  });
  await act(async () => { await Promise.resolve(); });
  const sent = api.editPolar.mock.calls;
  expect(sent.map((call) => call[1])).toEqual([{ type: "drag", bsp: 10 }, { type: "drag", bsp: 8.95 }]);
  expect(sent.every((call) => call[0] === 10 && call[3] === sent[0]![3])).toBe(true);
  expect(sent[0]![2]).toEqual([{ twa_index: 1, tws_index: 1 }]);
  // Selecting the node selected its cell in the table.
  expect(q(".view3d-edit-cell.selected")).not.toBeNull();

  await act(async () => (feature("edit:done") as HTMLButtonElement).click());
  expect(q(".view3d-edit")).toBeNull();
  expect(feature("view3d:tool-drag")).toBeNull();
});

it("takes only the flags when the scene held names its samples", async () => {
  await render(1);
  const first = api.polarScene.mock.results[0]!.value as Promise<ScenePacket>;
  await render(2);
  expect(api.polarScene).toHaveBeenLastCalledWith(null, await first);
});

it("never sends another project's samples key: a new project fetches the whole scene", async () => {
  await render(1, 1);
  await render(2, 1);
  expect(api.polarScene.mock.calls[1]![1]).not.toBeNull();
  await render(1, 2);
  expect(api.polarScene).toHaveBeenLastCalledWith(null, null);
});
