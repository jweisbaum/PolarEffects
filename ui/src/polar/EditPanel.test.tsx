// @vitest-environment happy-dom
/**
 * The edit panel (spec.md 10.4) against a mocked backend: the table shows
 * the surface with its edits marked, typed values and tools become edits in
 * Rust over the selected cells, and a track offers its statistic.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { EditSurface } from "../generated/EditSurface";
import type { ProjectSummary } from "../generated/ProjectSummary";

const api = vi.hoisted(() => ({ polarEditSurface: vi.fn(), editPolar: vi.fn(), setSegmentStatistic: vi.fn() }));
vi.mock("../ipc", () => ({ api }));

const { default: EditPanel, cellCode, parseSpeed } = await import("./EditPanel");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

const project: ProjectSummary = {
  id: 1, name: "P", path: null, dirty: false, revision: 1, boat_name: "", boat_notes: "",
  sources: [{ id: 30, kind: "track", label: "Alpha", colour: "#0000ff", visible: true, weight: 1, count: 12, used: 12, polar_file: null, orc: null, track: null, edits: 1 }],
  can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false,
};

const surface: EditSurface = {
  source_id: 30, kind: "track", twa: [60, 90], tws: [8, 12],
  source: [[null, null], [null, 6.81]], bsp: [[null, 5.5], [null, 6.81]],
  edited: [[false, true], [false, false]], excluded: [[false, false], [false, false]],
  count: [[3, 0], [0, 10]], spread: [[0.1, null], [null, 0.3]], statistic: "p90", min_samples: 5, edit_count: 1,
};

const onProject = vi.fn();
const onSelectCells = vi.fn();

async function render(selected: number[] = []) {
  await act(async () => root.render(
    <EditPanel project={project} sourceId={30} selected={new Set(selected)} hideOthers={false} onHideOthers={() => undefined}
      onSelectCells={onSelectCells} onProject={onProject} onDone={() => undefined} />,
  ));
  await act(async () => { await Promise.resolve(); });
}

const feature = (id: string) => host.querySelector<HTMLElement>(`[data-feature="${id}"]`)!;

beforeEach(() => {
  api.polarEditSurface.mockReset().mockResolvedValue(surface);
  api.editPolar.mockReset().mockResolvedValue(project);
  api.setSegmentStatistic.mockReset().mockResolvedValue(project);
  onProject.mockReset();
  onSelectCells.mockReset();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("reads typed speeds: a comma, an empty box, and nonsense", () => {
  expect(parseSpeed(" 6,25 ")).toBe(6.25);
  expect(parseSpeed("")).toBeNull();
  expect(parseSpeed("fast")).toBeUndefined();
  expect(parseSpeed("61")).toBeUndefined();
  expect(parseSpeed("-1")).toBeUndefined();
});

it("shows the surface with its edit marked and a track's counts in the tooltips", async () => {
  await render();
  const cells = [...host.querySelectorAll<HTMLTableCellElement>(".view3d-edit-cell")];
  expect(cells.map((c) => c.querySelector("input")!.value)).toEqual(["", "5.50", "", "6.81"]);
  expect(cells[1]!.classList.contains("edited")).toBe(true);
  expect(cells[3]!.querySelector("input")!.title).toBe("Segment value 6.81 kn · 10 samples, spread ±0.30 kn");
  expect(host.textContent).toContain("A cell needs 5 samples to have a value.");
  expect((feature("edit:reset-all") as HTMLButtonElement).disabled).toBe(false);
  expect((feature("edit:smooth") as HTMLButtonElement).disabled).toBe(true);
});

it("sends the tools over the selected cells, and the statistic", async () => {
  await render([cellCode(1, 1), cellCode(0, 1)]);
  const percent = feature("edit:scale-percent") as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(percent, "-4");
    percent.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => feature("edit:scale").click());
  expect(api.editPolar).toHaveBeenLastCalledWith(30, { type: "scale", percent: -4 }, [
    { twa_index: 1, tws_index: 1 }, { twa_index: 0, tws_index: 1 },
  ]);
  await act(async () => feature("edit:reset-all").click());
  expect(api.editPolar).toHaveBeenLastCalledWith(30, { type: "reset_all" }, []);
  const statistic = feature("edit:statistic") as HTMLSelectElement;
  await act(async () => {
    statistic.value = "median";
    statistic.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(api.setSegmentStatistic).toHaveBeenCalledWith(30, "median");
  expect(onProject).toHaveBeenCalled();
});
