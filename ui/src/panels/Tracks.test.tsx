// @vitest-environment happy-dom
/**
 * The Tracks section (spec.md 7.1, 7.3, 7.6): File… inspects the chosen
 * files, the dialog maps a CSV's columns and picks boats, Import sends the
 * answers to Rust, and a track's filters and derivation are edited through
 * their commands.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackSummary } from "../generated/TrackSummary";

const calls: [string, unknown][] = [];
const record = (name: string) => (...args: unknown[]) => { calls.push([name, args]); return Promise.resolve(responses[name]); };
const responses: Record<string, unknown> = {};
vi.mock("../ipc", () => ({
  api: new Proxy({}, { get: (_t, name: string) => record(name) }),
}));
vi.mock("../project/dialogs", () => ({ pickTrackFiles: () => Promise.resolve(["/races/log.csv", "/races/fleet.geojson"]) }));
const focus = vi.fn();
vi.mock("../selection", () => ({ focusMap: (f: unknown) => focus(f) }));

const { default: Tracks } = await import("./Tracks");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

const TRACK: TrackSummary = {
  origin: "file", boat_name: "Alpha", event_title: "fleet.geojson", start: 1_753_531_200, end: 1_753_617_600,
  samples: 120, filtered: 20, excluded: 0, used: 100, with_wind: 0, env_status: "not_fetched", max_gap_s: 10_800,
  prefer: "given", environment_filters: false,
  filters: { time_start: null, time_end: null, min_bsp: 1, max_bsp: null, max_heading_change: 30, heading_origin: "any", speed_origin: "any" },
};
const SOURCE: SourceSummary = {
  id: 5, kind: "track", label: "Alpha", colour: "#e15759", visible: true, weight: 1, count: 120, used: 100,
  polar_file: null, orc: null, track: TRACK,
};
const project = (sources: SourceSummary[]): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision: 1, boat_name: "", boat_notes: "", sources,
  can_undo: false, can_redo: false, undo_label: null, redo_label: null,
});

const CSV: TrackFileInspection = {
  path: "/races/log.csv", file: "log.csv", kind: "csv", boats: [], failure: {
    file: "log.csv", line: null, column: null, feature: null, reason: "no-mapping", message: "",
  },
  csv: {
    header: ["when", "lat", "lon"], rows: [["26/07/2025 12:00", "50", "-1"]], row_count: 1,
    mapping: { time: null, lat: 1, lon: 2, heading: null, speed: null, boat: null, time_format: "auto", custom_format: "", speed_unit: "kn" },
  },
};
const GEOJSON: TrackFileInspection = {
  path: "/races/fleet.geojson", file: "fleet.geojson", kind: "geojson", csv: null, failure: null,
  boats: [
    { name: "Alpha", fixes: 3, start: 1_753_531_200, end: 1_753_532_400 },
    { name: "Bravo", fixes: 3, start: 1_753_531_200, end: 1_753_532_400 },
  ],
};

const click = async (el: Element | null) => { await act(async () => { (el as HTMLElement).click(); }); };
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const q = (selector: string) => host.querySelector(selector);

beforeEach(() => {
  calls.length = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("lists a track with its boat, event, dates, samples used and environment status", async () => {
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  const text = q(".track-list")!.textContent!;
  expect(text).toContain("Alpha");
  expect(text).toContain("fleet.geojson · 2025-07-26 – 2025-07-27");
  expect(text).toContain("100 of 120 samples used");
  expect(text).toContain("Environment: not fetched");
  await click(q('[data-feature="tracks:show-on-map"]'));
  expect(focus).toHaveBeenCalledWith({ kind: "track", sourceId: 5 });
  // The tracker imports are there, and say they come later.
  expect((q('[data-feature="tracks:yellowbrick"]') as HTMLButtonElement).disabled).toBe(true);
});

it("edits the filters and the derivation through their commands", async () => {
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:filters"]'));
  const min = q('[data-feature="tracks:min-bsp"]') as HTMLInputElement;
  expect(min.value).toBe("1");
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(min, "3");
    min.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { min.blur(); min.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  expect(calls.find(([n]) => n === "setTrackFilters")?.[1]).toEqual([5, { ...TRACK.filters, min_bsp: 3 }]);

  const prefer = q('[data-feature="tracks:prefer"]') as HTMLSelectElement;
  await act(async () => {
    prefer.value = "derived";
    prefer.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(calls.find(([n]) => n === "setTrackDerivation")?.[1]).toEqual([5, 10_800, "derived"]);
  // The environment filters are there, and wait for the environment.
  expect((q('[data-feature="tracks:env-filters"]') as HTMLFieldSetElement).disabled).toBe(true);
});

it("maps a CSV, picks boats and imports them as one request", async () => {
  responses.inspectTrackFiles = [CSV, GEOJSON];
  responses.inspectCsvTrack = { ...CSV, failure: null, boats: [{ name: "", fixes: 1, start: 1_753_531_200, end: 1_753_531_200 }],
    csv: { ...CSV.csv!, mapping: { ...CSV.csv!.mapping, time: 0, time_format: "custom", custom_format: "%d/%m/%Y %H:%M" } } };
  responses.importTrackFiles = { project: project([SOURCE]), imported: [], failures: [] };
  const onProject = vi.fn();
  await act(async () => root.render(<Tracks project={project([])} onProject={onProject} />));
  await click(q('[data-feature="tracks:import-file"]'));
  await settle();
  expect(q("[role=dialog]")).not.toBeNull();
  // The CSV cannot import until its time column is chosen.
  expect(q(".modal-actions button.primary")!.textContent).toBe("Import 1 of 2 files");
  // The first column select is the time's.
  const time = q(".track-import-mapping select") as HTMLSelectElement;
  vi.useFakeTimers();
  await act(async () => {
    time.value = "0";
    time.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await act(async () => { vi.advanceTimersByTime(400); });
  vi.useRealTimers();
  await settle();
  expect(calls.find(([n]) => n === "inspectCsvTrack")?.[1]).toEqual([
    "/races/log.csv", { ...CSV.csv!.mapping, time: 0 },
  ]);
  expect(q(".modal-actions button.primary")!.textContent).toBe("Import");

  // Untick Bravo.
  const boxes = [...host.querySelectorAll<HTMLInputElement>(".track-import-boats input")];
  expect(boxes.length).toBe(2);
  await click(boxes[1]!);
  await click(q(".modal-actions button.primary"));
  await settle();
  const [files] = calls.find(([n]) => n === "importTrackFiles")![1] as [unknown[]];
  expect(files).toEqual([
    { path: "/races/log.csv", mapping: { ...CSV.csv!.mapping, time: 0, time_format: "custom", custom_format: "%d/%m/%Y %H:%M" }, boats: [""] },
    { path: "/races/fleet.geojson", mapping: null, boats: ["Alpha"] },
  ]);
  expect(onProject).toHaveBeenCalled();
  expect(q("[role=dialog]")).toBeNull();
});
